//! OIDC login: discovery, authorization-code flow with PKCE, strict token
//! verification, browser-bound flows and issuer+subject account mapping.
//!
//! Security rules baked in here:
//! - Every flow is bound to the browser that started it: /start sets a
//!   random HttpOnly SameSite=Lax binding cookie and stores only its hash.
//!   /callback must present the same binding (constant-time compared) and a
//!   missing or different browser is refused before the flow is consumed,
//!   so a stolen state/code pair is useless in another browser.
//! - Flow rows are single-use: consumption is DELETE ... RETURNING, so a
//!   replayed state is refused even across restarts and concurrent requests.
//! - Signature, issuer, audience, expiry and nonce are all validated by the
//!   openidconnect verifier; nothing is hand-rolled.
//! - Accounts are mapped by (issuer, subject) composite key, so a provider
//!   configuration change can never attach a subject to the wrong account.
//! - Provisioning requires an e-mail claim that the provider verified; an
//!   unverified e-mail is never used for identity or invitation matching.
//! - Provider-facing failures are reported with fixed, sanitized messages;
//!   raw error text, tokens and response bodies never reach the client.

use crate::auth::{create_session, session_cookie, CurrentUser};
use crate::{error_response, AppState};
use axum::extract::{Query, State};
use axum::http::{HeaderMap, StatusCode};
use axum::response::{IntoResponse, Response};
use axum::routing::get;
use axum::{Json, Router};
use businex_core::Error;
use chrono::{Duration, Utc};
use serde::Deserialize;
use serde_json::json;
use sha2::{Digest, Sha256};
use sqlx::Row;
use std::sync::Arc;
use uuid::Uuid;

use openidconnect::core::{CoreClient, CoreIdTokenClaims, CoreProviderMetadata, CoreResponseType};
use openidconnect::reqwest;
use openidconnect::{
    AuthenticationFlow, AuthorizationCode, ClientId, ClientSecret, CsrfToken, EndpointMaybeSet,
    EndpointNotSet, EndpointSet, IssuerUrl, Nonce, PkceCodeChallenge, PkceCodeVerifier,
    RedirectUrl, Scope, TokenResponse,
};

/// Client produced by discovery: authorization endpoint is always known,
/// token and userinfo endpoints stay optional until the metadata says so.
type DiscoveredClient = CoreClient<
    EndpointSet,
    EndpointNotSet,
    EndpointNotSet,
    EndpointNotSet,
    EndpointMaybeSet,
    EndpointMaybeSet,
>;

const BIND_COOKIE: &str = "businex_oidc_bind";
const FLOW_TTL_MINS: i64 = 10;

/// Confidential client material. Debug never exposes its contents.
#[derive(Clone)]
pub struct ClientCred(String);

impl ClientCred {
    pub fn new(value: String) -> Self {
        Self(value)
    }
    fn expose(&self) -> String {
        self.0.clone()
    }
}

impl std::fmt::Debug for ClientCred {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("[redacted]")
    }
}

/// Runtime OIDC configuration, built once at startup from the environment.
#[derive(Clone)]
pub struct OidcConfig {
    pub issuer: String,
    pub client_id: String,
    /// Confidential client credential, loaded from a file. Never logged.
    pub auth_material: ClientCred,
    pub redirect_url: String,
}

// Manual Debug so the credential is redacted even in trace output.
impl std::fmt::Debug for OidcConfig {
    fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
        f.debug_struct("OidcConfig")
            .field("issuer", &self.issuer)
            .field("client_id", &self.client_id)
            .field("auth_material", &"[redacted]")
            .field("redirect_url", &self.redirect_url)
            .finish()
    }
}

impl OidcConfig {
    /// Stable mapping key for the issuer. Trailing slashes are normalized so
    /// the same provider always maps to the same key.
    pub fn issuer_key(&self) -> String {
        self.issuer.trim().trim_end_matches('/').to_string()
    }
}

/// Fixed, sanitized configuration failures. Every message is a constant so
/// error output can never leak secret material or file contents.
#[derive(Debug, PartialEq, Eq)]
pub enum OidcConfigError {
    /// Some settings are present but others are missing or blank.
    Partial,
    /// The credential file exists but cannot be read.
    CredentialUnreadable,
    /// The credential file is readable but holds no material.
    CredentialEmpty,
}

impl std::fmt::Display for OidcConfigError {
    fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
        let text = match self {
            Self::Partial => "oidc configuration incomplete",
            Self::CredentialUnreadable => "oidc credential file is not readable",
            Self::CredentialEmpty => "oidc credential file is empty",
        };
        f.write_str(text)
    }
}

impl std::error::Error for OidcConfigError {}

const OIDC_ISSUER_KEY: &str = "BUSINEX_OIDC_ISSUER";
const OIDC_CLIENT_ID_KEY: &str = "BUSINEX_OIDC_CLIENT_ID";
const OIDC_REDIRECT_KEY: &str = "BUSINEX_OIDC_REDIRECT_URL";
const OIDC_CRED_FILE_KEY: &str = "BUSINEX_OIDC_CRED_FILE";

/// Build the OIDC runtime from the process environment.
///
/// `Ok(None)` means every setting is absent and the login routes stay
/// disabled. Any partial or invalid configuration is a hard startup error:
/// a half-configured provider must never silently degrade to "disabled".
pub fn load_from_env() -> Result<Option<OidcRuntime>, OidcConfigError> {
    load_with(
        |key| std::env::var(key).ok(),
        |path| std::fs::read_to_string(path),
    )
}

/// Testable core of [`load_from_env`]: lookup and credential reading are
/// injected so every branch is covered without touching the real environment.
fn load_with<G, R>(get: G, read_credential: R) -> Result<Option<OidcRuntime>, OidcConfigError>
where
    G: Fn(&str) -> Option<String>,
    R: Fn(&str) -> Result<String, std::io::Error>,
{
    let issuer = get(OIDC_ISSUER_KEY);
    let client_id = get(OIDC_CLIENT_ID_KEY);
    let redirect_url = get(OIDC_REDIRECT_KEY);
    let cred_file = get(OIDC_CRED_FILE_KEY);
    if [&issuer, &client_id, &redirect_url, &cred_file]
        .iter()
        .all(|value| value.is_none())
    {
        return Ok(None);
    }
    let (Some(issuer), Some(client_id), Some(redirect_url), Some(cred_file)) =
        (issuer, client_id, redirect_url, cred_file)
    else {
        return Err(OidcConfigError::Partial);
    };
    for value in [&issuer, &client_id, &redirect_url, &cred_file] {
        if value.trim().is_empty() {
            return Err(OidcConfigError::Partial);
        }
    }
    let material = read_credential(&cred_file)
        .map_err(|_| OidcConfigError::CredentialUnreadable)?
        .trim()
        .to_string();
    if material.is_empty() {
        return Err(OidcConfigError::CredentialEmpty);
    }
    Ok(Some(OidcRuntime::new(OidcConfig {
        issuer,
        client_id,
        auth_material: ClientCred::new(material),
        redirect_url,
    })))
}

/// Shared discovery cache and HTTP client.
#[derive(Clone)]
pub struct OidcRuntime {
    pub config: OidcConfig,
    pub http: reqwest::Client,
    provider: Arc<tokio::sync::RwLock<Option<Arc<CoreProviderMetadata>>>>,
}

impl std::fmt::Debug for OidcRuntime {
    fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
        f.debug_struct("OidcRuntime")
            .field("config", &self.config)
            .finish()
    }
}

impl OidcRuntime {
    pub fn new(config: OidcConfig) -> Self {
        let http = reqwest::Client::builder()
            // Following redirects on server-side calls opens SSRF holes.
            .redirect(reqwest::redirect::Policy::none())
            .build()
            .expect("oidc http client");
        OidcRuntime {
            config,
            http,
            provider: Arc::new(tokio::sync::RwLock::new(None)),
        }
    }

    /// Discover (and cache) provider metadata including the JWKS used later
    /// for signature verification. Failure is reported without response text.
    async fn provider(&self) -> Result<Arc<CoreProviderMetadata>, Response> {
        if let Some(cached) = self.provider.read().await.clone() {
            return Ok(cached);
        }
        let issuer = IssuerUrl::new(self.config.issuer.clone()).map_err(|_| {
            error_response(&Error::Invalid {
                message: "oidc issuer is not a valid url".into(),
            })
        })?;
        let metadata = CoreProviderMetadata::discover_async(issuer, &self.http)
            .await
            .map_err(|_| {
                tracing::warn!(stage = "discovery", "oidc provider discovery failed");
                error_response(&Error::Internal("oidc discovery failed".into()))
            })?;
        let shared = Arc::new(metadata);
        let mut guard = self.provider.write().await;
        *guard = Some(shared.clone());
        Ok(shared)
    }

    /// Build the OIDC client from discovered metadata.
    fn client(&self, provider: &CoreProviderMetadata) -> Result<DiscoveredClient, Response> {
        let redirect = RedirectUrl::new(self.config.redirect_url.clone()).map_err(|_| {
            error_response(&Error::Invalid {
                message: "oidc redirect url is invalid".into(),
            })
        })?;
        Ok(CoreClient::from_provider_metadata(
            provider.clone(),
            ClientId::new(self.config.client_id.clone()),
            Some(ClientSecret::new(self.config.auth_material.expose())),
        )
        .set_redirect_uri(redirect))
    }
}

pub fn router() -> Router<AppState> {
    Router::new()
        .route("/api/auth/oidc/start", get(start))
        .route("/api/auth/oidc/callback", get(callback))
}

/// Constant-time equality for fixed-length digests.
fn ct_eq(a: &[u8], b: &[u8]) -> bool {
    if a.len() != b.len() {
        return false;
    }
    let mut diff = 0u8;
    for (x, y) in a.iter().zip(b.iter()) {
        diff |= x ^ y;
    }
    diff == 0
}

fn sha256_hex(value: &str) -> String {
    hex::encode(Sha256::digest(value.as_bytes()))
}

/// Only a same-origin relative path survives; backslashes, control
/// characters, absolute urls and network-path references all fall back.
pub(crate) fn safe_return_to(raw: Option<String>) -> String {
    let Some(candidate) = raw else {
        return "/".to_string();
    };
    let candidate = candidate.trim();
    let safe = !candidate.is_empty()
        && candidate.len() <= 2048
        && candidate.starts_with('/')
        && !candidate.starts_with("//")
        && !candidate.starts_with("/\\")
        && !candidate.contains('\\')
        && !candidate.chars().any(char::is_control);
    if safe {
        candidate.to_string()
    } else {
        "/".to_string()
    }
}

fn binding_cookie(value: &str, max_age_secs: i64, secure: bool) -> String {
    format!(
        "{}={}; Path=/; HttpOnly; SameSite=Lax; Max-Age={}{}",
        BIND_COOKIE,
        value,
        max_age_secs,
        if secure { "; Secure" } else { "" }
    )
}

fn cookie_value(headers: &HeaderMap, name: &str) -> Option<String> {
    for value in headers.get_all("cookie") {
        let Ok(text) = value.to_str() else { continue };
        for pair in text.split(';') {
            let pair = pair.trim();
            if let Some(rest) = pair.strip_prefix(name) {
                if let Some(val) = rest.strip_prefix('=') {
                    return Some(val.to_string());
                }
            }
        }
    }
    None
}

#[derive(Deserialize)]
pub struct StartQuery {
    return_to: Option<String>,
}

/// Step 1: create a browser-bound pending flow and return the authorization
/// url the browser should visit.
async fn start(
    State(state): State<AppState>,
    headers: HeaderMap,
    Query(query): Query<StartQuery>,
) -> Result<Response, Response> {
    let oidc = state.oidc.as_ref().ok_or_else(|| {
        error_response(&Error::NotFound {
            entity: "oidc configuration".into(),
        })
    })?;
    let provider = oidc.provider().await?;
    let client = oidc.client(&provider)?;

    let (pkce_challenge, pkce_verifier) = PkceCodeChallenge::new_random_sha256();
    let (auth_url, csrf_state, nonce) = client
        .authorize_url(
            AuthenticationFlow::<CoreResponseType>::AuthorizationCode,
            CsrfToken::new_random,
            Nonce::new_random,
        )
        .add_scope(Scope::new("openid".to_string()))
        .add_scope(Scope::new("email".to_string()))
        .add_scope(Scope::new("profile".to_string()))
        .set_pkce_challenge(pkce_challenge)
        .url();

    // Browser binding: a fresh random value in an HttpOnly cookie; only its
    // hash is stored, so a database leak cannot impersonate a browser.
    let binding = hex::encode(rand::random::<[u8; 32]>());
    let state_hash = sha256_hex(csrf_state.secret());
    let browser_hash = sha256_hex(&binding);
    sqlx::query(
        "INSERT INTO oidc_flows (state_hash, browser_hash, nonce, code_verifier, return_to, expires_at)\n
         VALUES ($1, $2, $3, $4, $5, $6)",
    )
    .bind(&state_hash)
    .bind(&browser_hash)
    .bind(nonce.secret().clone())
    .bind(pkce_verifier.secret().clone())
    .bind(safe_return_to(query.return_to))
    .bind(Utc::now() + Duration::minutes(FLOW_TTL_MINS))
    .execute(&state.pool)
    .await
    .map_err(|_| error_response(&Error::Internal("could not store oidc flow".into())))?;
    let _ = sqlx::query("DELETE FROM oidc_flows WHERE expires_at < now()")
        .execute(&state.pool)
        .await;

    let ttl = FLOW_TTL_MINS * 60;
    let cookie = binding_cookie(&binding, ttl, state.config.cookie_secure);
    let mut resp = Json(json!({ "authorization_url": auth_url.to_string() })).into_response();
    resp.headers_mut()
        .insert("set-cookie", cookie.parse().expect("cookie header"));
    let _ = headers;
    Ok(resp)
}

#[derive(Deserialize)]
pub struct CallbackQuery {
    code: Option<String>,
    state: Option<String>,
    error: Option<String>,
}

/// Step 2: verify the browser, exchange the code, verify the token, map the
/// subject and establish a normal server session.
async fn callback(
    State(state): State<AppState>,
    headers: HeaderMap,
    Query(query): Query<CallbackQuery>,
) -> Result<Response, Response> {
    let oidc = state.oidc.as_ref().ok_or_else(|| {
        error_response(&Error::NotFound {
            entity: "oidc configuration".into(),
        })
    })?;
    if query.error.is_some() {
        tracing::warn!(stage = "authorize", "oidc provider reported an error");
        return Err(error_response(&Error::Forbidden {
            action: "oidc_login".into(),
            reason: "provider returned an error".into(),
        }));
    }
    let code = query.code.ok_or_else(|| {
        error_response(&Error::Invalid {
            message: "missing code".into(),
        })
    })?;
    let state_param = query.state.ok_or_else(|| {
        error_response(&Error::Invalid {
            message: "missing state".into(),
        })
    })?;

    let state_hash = sha256_hex(&state_param);
    let row = sqlx::query(
        "SELECT nonce, code_verifier, return_to, browser_hash, expires_at\n
         FROM oidc_flows WHERE state_hash = $1",
    )
    .bind(&state_hash)
    .fetch_optional(&state.pool)
    .await
    .map_err(|_| error_response(&Error::Internal("could not read oidc flow".into())))?
    .ok_or_else(|| {
        error_response(&Error::Forbidden {
            action: "oidc_login".into(),
            reason: "unknown or expired state".into(),
        })
    })?;
    let stored_nonce: String = row.get(0);
    let code_verifier: String = row.get(1);
    let return_to: String = row.get(2);
    let browser_hash: String = row.get(3);
    let expires_at: chrono::DateTime<Utc> = row.get(4);
    if expires_at <= Utc::now() {
        let _ = sqlx::query("DELETE FROM oidc_flows WHERE state_hash = $1")
            .bind(&state_hash)
            .execute(&state.pool)
            .await;
        return Err(error_response(&Error::Forbidden {
            action: "oidc_login".into(),
            reason: "unknown or expired state".into(),
        }));
    }

    // Browser binding check happens BEFORE consuming the flow so a victim
    // clicking an attacker link cannot burn the attacker flow and cannot
    // redeem it; the flow stays usable by its originating browser only.
    let presented = cookie_value(&headers, BIND_COOKIE).ok_or_else(|| {
        error_response(&Error::Forbidden {
            action: "oidc_login".into(),
            reason: "flow is bound to a different browser".into(),
        })
    })?;
    if !ct_eq(sha256_hex(&presented).as_bytes(), browser_hash.as_bytes()) {
        return Err(error_response(&Error::Forbidden {
            action: "oidc_login".into(),
            reason: "flow is bound to a different browser".into(),
        }));
    }

    // Single-use consume; a concurrent duplicate loses the race here.
    let consumed = sqlx::query("DELETE FROM oidc_flows WHERE state_hash = $1")
        .bind(&state_hash)
        .execute(&state.pool)
        .await
        .map_err(|_| error_response(&Error::Internal("could not consume oidc flow".into())))?;
    if consumed.rows_affected() != 1 {
        return Err(error_response(&Error::Forbidden {
            action: "oidc_login".into(),
            reason: "unknown or expired state".into(),
        }));
    }

    let provider = oidc.provider().await?;
    let client = oidc.client(&provider)?;
    let exchange = client
        .exchange_code(AuthorizationCode::new(code))
        .map_err(|_| {
            error_response(&Error::Internal("oidc not configured for code flow".into()))
        })?;
    let tok_resp = exchange
        .set_pkce_verifier(PkceCodeVerifier::new(code_verifier))
        .request_async(&oidc.http)
        .await
        .map_err(|_| {
            tracing::warn!(stage = "exchange", "oidc code exchange failed");
            error_response(&Error::Forbidden {
                action: "oidc_login".into(),
                reason: "code exchange failed".into(),
            })
        })?;
    let idt = tok_resp.id_token().ok_or_else(|| {
        error_response(&Error::Forbidden {
            action: "oidc_login".into(),
            reason: "provider returned no identity".into(),
        })
    })?;

    // Library verification: signature, issuer, audience, expiry and nonce.
    let nonce = Nonce::new(stored_nonce);
    let claims: &CoreIdTokenClaims =
        idt.claims(&client.id_token_verifier(), &nonce)
            .map_err(|_| {
                tracing::warn!(stage = "verify", "oidc token verification failed");
                error_response(&Error::Forbidden {
                    action: "oidc_login".into(),
                    reason: "identity verification failed".into(),
                })
            })?;

    let subject = claims.subject().to_string();
    let issuer = oidc.config.issuer_key();

    // Composite mapping: identity is (issuer, subject), never subject alone.
    let existing: Option<(Uuid, String, String)> = sqlx::query_as(
        "SELECT u.id, u.email, u.name\n
         FROM oidc_identities oi JOIN users u ON u.id = oi.user_id\n
         WHERE oi.issuer = $1 AND oi.subject = $2",
    )
    .bind(&issuer)
    .bind(&subject)
    .fetch_optional(&state.pool)
    .await
    .map_err(|_| error_response(&Error::Internal("database error".into())))?;

    let user = match existing {
        Some((id, email, name)) => CurrentUser {
            id,
            email,
            name,
            session_id: Uuid::nil(),
        },
        None => {
            // Provisioning needs a provider-VERIFIED e-mail; unverified
            // addresses are never used for identity or invitation matching.
            let verified_email = claims
                .email()
                .filter(|_| claims.email_verified() == Some(true))
                .map(|e| e.to_string().to_lowercase());
            let email = verified_email.ok_or_else(|| {
                error_response(&Error::Forbidden {
                    action: "oidc_login".into(),
                    reason: "provider sent no verified e-mail claim".into(),
                })
            })?;
            let display_name = claims
                .preferred_username()
                .map(|u| u.to_string())
                .unwrap_or_else(|| email.clone());
            let id = Uuid::new_v4();
            let mut tx = state
                .pool
                .begin()
                .await
                .map_err(|_| error_response(&Error::Internal("database error".into())))?;
            let inserted = sqlx::query("INSERT INTO users (id, email, name) VALUES ($1, $2, $3)")
                .bind(id)
                .bind(&email)
                .bind(&display_name)
                .execute(&mut *tx)
                .await;
            match inserted {
                Ok(_) => {}
                Err(sqlx::Error::Database(db_err)) if db_err.constraint().is_some() => {
                    return Err(error_response(&Error::Conflict {
                        message: "an account with this e-mail already exists; \
                                  sign in with the existing method first"
                            .into(),
                    }));
                }
                Err(_) => {
                    return Err(error_response(&Error::Internal("database error".into())));
                }
            }
            sqlx::query(
                "INSERT INTO oidc_identities (issuer, subject, user_id) VALUES ($1, $2, $3)",
            )
            .bind(&issuer)
            .bind(&subject)
            .bind(id)
            .execute(&mut *tx)
            .await
            .map_err(|_| error_response(&Error::Internal("database error".into())))?;
            tx.commit()
                .await
                .map_err(|_| error_response(&Error::Internal("database error".into())))?;
            CurrentUser {
                id,
                email,
                name: display_name,
                session_id: Uuid::nil(),
            }
        }
    };

    let ua = headers
        .get("user-agent")
        .and_then(|v| v.to_str().ok())
        .map(String::from);
    let ttl = Duration::seconds(state.config.session_ttl_secs);
    let (sess_val, _expires) = create_session(&state.pool, user.id, ua.as_deref(), ttl)
        .await
        .map_err(|_| error_response(&Error::Internal("session error".into())))?;
    let cookie = session_cookie(
        &sess_val,
        state.config.session_ttl_secs,
        state.config.cookie_secure,
    );
    let mut resp = (
        StatusCode::OK,
        Json(json!({
            "user": {"id": user.id, "email": user.email, "name": user.name},
            "return_to": safe_return_to(Some(return_to)),
        })),
    )
        .into_response();
    resp.headers_mut()
        .insert("set-cookie", cookie.parse().expect("cookie header"));
    Ok(resp)
}

#[cfg(test)]
mod tests {
    use super::{
        ct_eq, load_with, safe_return_to, OidcConfigError, OIDC_CLIENT_ID_KEY, OIDC_CRED_FILE_KEY,
        OIDC_ISSUER_KEY, OIDC_REDIRECT_KEY,
    };

    fn lookup(pairs: Vec<(&str, String)>) -> impl Fn(&str) -> Option<String> {
        let map: std::collections::HashMap<String, String> = pairs
            .into_iter()
            .map(|(key, value)| (key.to_string(), value))
            .collect();
        move |key| map.get(key).cloned()
    }

    fn full_settings() -> Vec<(&'static str, String)> {
        vec![
            (OIDC_ISSUER_KEY, "https://idp.example".to_string()),
            (OIDC_CLIENT_ID_KEY, "client-1".to_string()),
            (
                OIDC_REDIRECT_KEY,
                "https://app.example/auth/oidc/callback".to_string(),
            ),
            (OIDC_CRED_FILE_KEY, "/tmp/oidc-cred".to_string()),
        ]
    }

    #[tokio::test]
    async fn config_absent_disables_oidc() {
        let no_read = |_: &str| -> Result<String, std::io::Error> {
            unreachable!("credential must not be read")
        };
        let runtime = load_with(|_| None, no_read);
        assert!(matches!(runtime, Ok(None)));
    }

    #[tokio::test]
    async fn config_partial_is_rejected() {
        let mut settings = full_settings();
        settings.pop();
        let runtime = load_with(lookup(settings), |_| Ok(String::from("x")));
        assert!(matches!(runtime, Err(OidcConfigError::Partial)));
    }

    #[tokio::test]
    async fn config_blank_value_is_rejected() {
        let mut settings = full_settings();
        settings[0].1 = "   ".to_string();
        let runtime = load_with(lookup(settings), |_| Ok(String::from("x")));
        assert!(matches!(runtime, Err(OidcConfigError::Partial)));
    }

    #[tokio::test]
    async fn config_unreadable_credential_is_rejected() {
        let runtime = load_with(lookup(full_settings()), |_| {
            Err(std::io::Error::new(std::io::ErrorKind::NotFound, "gone"))
        });
        assert!(matches!(
            runtime,
            Err(OidcConfigError::CredentialUnreadable)
        ));
    }

    #[tokio::test]
    async fn config_empty_credential_is_rejected() {
        let runtime = load_with(lookup(full_settings()), |_| Ok(String::from(" \n")));
        assert!(matches!(runtime, Err(OidcConfigError::CredentialEmpty)));
    }

    #[tokio::test]
    async fn config_valid_builds_runtime_and_redacts_credential() {
        let material = String::from("super-secret-material");
        let runtime = load_with(lookup(full_settings()), move |_| Ok(material.clone()))
            .expect("valid config")
            .expect("runtime");
        assert_eq!(runtime.config.issuer_key(), "https://idp.example");
        let debug = format!("{:?}", runtime);
        assert!(
            !debug.contains("super-secret-material"),
            "credential leaked: {}",
            debug
        );
    }

    #[test]
    fn return_to_rejects_escapes_and_redirects() {
        assert_eq!(safe_return_to(None), "/");
        assert_eq!(safe_return_to(Some("/dashboard".into())), "/dashboard");
        assert_eq!(safe_return_to(Some("//evil.example".into())), "/");
        assert_eq!(safe_return_to(Some("/\\\\evil.example".into())), "/");
        assert_eq!(safe_return_to(Some("\\\\evil.example".into())), "/");
        assert_eq!(safe_return_to(Some("https://evil.example".into())), "/");
        assert_eq!(safe_return_to(Some("/ok\u{0007}path".into())), "/");
        assert_eq!(safe_return_to(Some("".into())), "/");
    }

    #[test]
    fn constant_time_eq_matches_only_equal_digests() {
        let a = [7u8; 32];
        let mut b = a;
        assert!(ct_eq(&a, &b));
        b[31] ^= 1;
        assert!(!ct_eq(&a, &b));
        assert!(!ct_eq(&a, &[0u8; 16]));
    }
}
