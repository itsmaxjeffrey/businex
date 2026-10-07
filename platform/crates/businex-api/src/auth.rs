//! Authentication: passwords, server sessions and the current-user binding.
//!
//! Session tokens are random 256-bit values; only their SHA-256 hash is stored.
//! Cookies are HttpOnly + SameSite=Strict. The actor identity and company
//! context used for authorization always come from this server-side session and
//! a verified membership lookup, never from client input.

use argon2::password_hash::rand_core::OsRng;
use argon2::password_hash::{PasswordHash, PasswordHasher, PasswordVerifier, SaltString};
use argon2::Argon2;
use axum::http::HeaderMap;
use chrono::{DateTime, Duration, Utc};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use sqlx::PgPool;
use uuid::Uuid;

pub const SESSION_COOKIE: &str = "businex_session";

#[derive(Debug, thiserror::Error)]
pub enum AuthError {
    #[error("invalid credentials")]
    InvalidCredentials,
    #[error("session expired or revoked")]
    SessionExpired,
    #[error("no session")]
    NoSession,
    #[error("database error")]
    Db(#[from] sqlx::Error),
    #[error("invalid input: {0}")]
    Invalid(String),
}

/// Argon2 is deliberately expensive; all password work runs on the blocking
/// pool through these async wrappers, bounded so a burst of logins cannot
/// exhaust the runtime or the machine.
const MAX_CONCURRENT_PASSWORD_OPS: usize = 4;

fn password_slots() -> &'static tokio::sync::Semaphore {
    use std::sync::OnceLock;
    static SLOTS: OnceLock<tokio::sync::Semaphore> = OnceLock::new();
    SLOTS.get_or_init(|| tokio::sync::Semaphore::new(MAX_CONCURRENT_PASSWORD_OPS))
}

/// Hash a password with Argon2id on the blocking pool. Each hash embeds its
/// own random salt.
pub async fn hash_password(password: String) -> Result<String, AuthError> {
    let permit = password_slots()
        .acquire()
        .await
        .map_err(|_| AuthError::Invalid("password service unavailable".into()))?;
    let result = tokio::task::spawn_blocking(move || -> Result<String, AuthError> {
        let salt = SaltString::generate(&mut OsRng);
        let hash = Argon2::default()
            .hash_password(password.as_bytes(), &salt)
            .map_err(|_| AuthError::Invalid("password hashing failed".into()))?;
        Ok(hash.to_string())
    })
    .await
    .map_err(|_| AuthError::Invalid("password service unavailable".into()))?;
    drop(permit);
    result
}

/// Verify a password with Argon2id on the blocking pool.
pub async fn verify_password(password: String, stored: String) -> Result<bool, AuthError> {
    let permit = password_slots()
        .acquire()
        .await
        .map_err(|_| AuthError::Invalid("password service unavailable".into()))?;
    let result = tokio::task::spawn_blocking(move || -> bool {
        PasswordHash::new(&stored)
            .map(|parsed| {
                Argon2::default()
                    .verify_password(password.as_bytes(), &parsed)
                    .is_ok()
            })
            .unwrap_or(false)
    })
    .await
    .map_err(|_| AuthError::Invalid("password service unavailable".into()))?;
    drop(permit);
    Ok(result)
}

fn hash_token(token: &str) -> String {
    hex::encode(Sha256::digest(token.as_bytes()))
}

#[derive(Debug, Clone, sqlx::FromRow)]
pub struct UserRow {
    pub id: Uuid,
    pub email: String,
    pub name: String,
    pub password_hash: Option<String>,
}

/// The authenticated principal for a request.
#[derive(Debug, Clone)]
pub struct CurrentUser {
    pub id: Uuid,
    pub email: String,
    pub name: String,
    pub session_id: Uuid,
}

/// Create a server session and return the raw token (sent once, in the cookie).
pub async fn create_session(
    pool: &PgPool,
    user_id: Uuid,
    user_agent: Option<&str>,
    ttl: Duration,
) -> Result<(String, DateTime<Utc>), AuthError> {
    let token = hex::encode(rand::random::<[u8; 32]>());
    let expires_at = Utc::now() + ttl;
    sqlx::query(
        r#"INSERT INTO sessions (id, user_id, token_hash, user_agent, expires_at)
           VALUES ($1, $2, $3, $4, $5)"#,
    )
    .bind(Uuid::new_v4())
    .bind(user_id)
    .bind(hash_token(&token))
    .bind(user_agent)
    .bind(expires_at)
    .execute(pool)
    .await?;
    Ok((token, expires_at))
}

/// Resolve a raw session token to its user. Expired or revoked sessions are
/// rejected and removed from consideration.
pub async fn user_from_token(pool: &PgPool, token: &str) -> Result<(CurrentUser, Uuid), AuthError> {
    let row: (Uuid, Uuid, String, String, DateTime<Utc>, Option<DateTime<Utc>>) = sqlx::query_as(
        r#"
        SELECT s.id, u.id, u.email, u.name, s.expires_at, s.revoked_at
        FROM sessions s JOIN users u ON u.id = s.user_id
        WHERE s.token_hash = $1
        "#,
    )
    .bind(hash_token(token))
    .fetch_optional(pool)
    .await?
    .ok_or(AuthError::NoSession)?;
    let (session_id, user_id, email, name, expires_at, revoked_at) = row;
    if revoked_at.is_some() || expires_at <= Utc::now() {
        return Err(AuthError::SessionExpired);
    }
    sqlx::query("UPDATE sessions SET last_seen_at = now() WHERE id = $1")
        .bind(session_id)
        .execute(pool)
        .await?;
    Ok((
        CurrentUser {
            id: user_id,
            email,
            name,
            session_id,
        },
        session_id,
    ))
}

pub async fn revoke_session(pool: &PgPool, session_id: Uuid) -> Result<(), AuthError> {
    sqlx::query("UPDATE sessions SET revoked_at = now() WHERE id = $1")
        .bind(session_id)
        .execute(pool)
        .await?;
    Ok(())
}

/// Extract the session token from the Cookie header.
pub fn token_from_headers(headers: &HeaderMap) -> Option<String> {
    let cookie = headers.get("cookie")?.to_str().ok()?;
    for part in cookie.split(';') {
        let mut kv = part.trim().splitn(2, '=');
        if let (Some(k), Some(v)) = (kv.next(), kv.next()) {
            if k == SESSION_COOKIE {
                return Some(v.to_string());
            }
        }
    }
    None
}

/// Build the Set-Cookie value for a session token.
pub fn session_cookie(token: &str, max_age_secs: i64, secure: bool) -> String {
    format!(
        "{}={}; Path=/; HttpOnly; SameSite=Strict; Max-Age={}{}",
        SESSION_COOKIE,
        token,
        max_age_secs,
        if secure { "; Secure" } else { "" }
    )
}

#[derive(Debug, Clone, Serialize, Deserialize, sqlx::FromRow)]
pub struct MembershipRow {
    pub company_id: Uuid,
    pub role: String,
    pub scope: serde_json::Value,
}
