//! Identity, tenancy and membership API routes.
//!
//! Every company-scoped request re-checks the verified membership through the
//! narrow SECURITY DEFINER lookup and builds the authorization from that row.
//! Tenant identity is never taken from the request body.

use crate::auth::{
    create_session, hash_password, revoke_session, session_cookie, token_from_headers,
    user_from_token, verify_password, AuthError, CurrentUser,
};
use crate::ratelimit::RateDecision;
use crate::{error_response, AppState};
use axum::extract::{Path, State};
use axum::http::{HeaderMap, HeaderValue, StatusCode};
use axum::response::{IntoResponse, Response};
use axum::routing::{get, patch, post};
use axum::{Json, Router};
use businex_core::{ActionGrant, Actor, Authorization, Error, Permission, ResourceScope, Role};
use chrono::{Duration, Utc};
use serde::{Deserialize, Serialize};
use serde_json::json;
use sha2::{Digest, Sha256};
use sqlx::Row;
use uuid::Uuid;

pub fn router() -> Router<AppState> {
    Router::new()
        .route("/api/auth/register", post(register))
        .route("/api/auth/login", post(login))
        .route("/api/auth/logout", post(logout))
        .route("/api/auth/me", get(me))
        .route("/api/companies", get(my_companies).post(create_company))
        .route("/api/companies/{id}/members", get(list_members))
        .route(
            "/api/companies/{id}/members/{user_id}",
            patch(update_member).delete(remove_member),
        )
        .route(
            "/api/companies/{id}/grants",
            get(list_grants).post(create_grant),
        )
        .route(
            "/api/companies/{id}/grants/{grant_id}",
            axum::routing::delete(revoke_grant),
        )
        .route("/api/companies/{id}/invitations", post(create_invitation))
        .route("/api/invitations/accept", post(accept_invitation))
}

fn map_auth(err: AuthError) -> Response {
    match err {
        AuthError::InvalidCredentials | AuthError::NoSession | AuthError::SessionExpired => {
            error_response(&Error::Forbidden {
                action: "authenticate".into(),
                reason: "invalid or expired session".into(),
            })
        }
        AuthError::Invalid(message) => error_response(&Error::Invalid { message }),
        AuthError::Db(_) => error_response(&Error::Internal("database error".into())),
    }
}

pub(crate) async fn current_user(
    state: &AppState,
    headers: &HeaderMap,
) -> Result<CurrentUser, Response> {
    let token = token_from_headers(headers).ok_or_else(|| map_auth(AuthError::NoSession))?;
    let (user, _) = user_from_token(&state.pool, &token)
        .await
        .map_err(map_auth)?;
    Ok(user)
}

/// Load the verified membership and build the authorization from it. The
/// stored scope: empty resources means the membership's role applies across
/// the company (explicit membership decision); non-empty resources narrow it.
pub(crate) async fn authorize(
    state: &AppState,
    user: &CurrentUser,
    company_id: Uuid,
    action: Permission,
    resource: &str,
) -> Result<Authorization, Response> {
    let row: Option<(String, serde_json::Value)> =
        sqlx::query_as("SELECT role, scope FROM businex.membership_for($1, $2)")
            .bind(user.id)
            .bind(company_id)
            .fetch_optional(&state.pool)
            .await
            .map_err(|_| error_response(&Error::Internal("database error".into())))?;
    let (role_str, scope) = row.ok_or_else(|| {
        error_response(&Error::Forbidden {
            action: action.as_str().into(),
            reason: "no membership in this company".into(),
        })
    })?;
    let role = Role::parse(&role_str)
        .ok_or_else(|| error_response(&Error::Internal("membership has unknown role".into())))?;
    let resources: Vec<String> = scope
        .get("resources")
        .and_then(|v| v.as_array())
        .map(|a| {
            a.iter()
                .filter_map(|v| v.as_str().map(String::from))
                .collect()
        })
        .unwrap_or_default();
    let resource_scope = if resources.is_empty() {
        ResourceScope::all()
    } else {
        // A membership scope narrows the member to listed resources; the
        // allowed actions are exactly the role's permissions on those
        // resources, never more.
        ResourceScope::actions(
            role.permissions()
                .into_iter()
                .flat_map(|p| {
                    resources
                        .iter()
                        .map(move |r| ActionGrant::new(p, r.clone()))
                })
                .collect(),
        )
    };
    let auth = Authorization {
        actor: Actor::User { id: user.id },
        company_id,
        role,
        scope: resource_scope,
    };
    auth.check(action, resource)
        .map_err(|e| error_response(&e))?;
    Ok(auth)
}

pub(crate) async fn audit(
    state: &AppState,
    auth: &Authorization,
    action: &str,
    entity_type: &str,
    entity_id: Option<Uuid>,
    meta: serde_json::Value,
) {
    let ctx = businex_db::CompanyContext::with_actor(
        auth.company_id,
        match auth.actor {
            Actor::User { id } => id,
            _ => Uuid::nil(),
        },
    );
    if let Ok(mut tx) = businex_db::begin_company_tx(&state.pool, ctx).await {
        let _ = sqlx::query(
            "INSERT INTO audit_log (id, company_id, actor_type, actor_id, action, entity_type, entity_id, meta)
             VALUES ($1, $2, 'user', $3, $4, $5, $6, $7)",
        )
        .bind(Uuid::new_v4())
        .bind(auth.company_id)
        .bind(match auth.actor {
            Actor::User { id } => Some(id),
            _ => None,
        })
        .bind(action)
        .bind(entity_type)
        .bind(entity_id)
        .bind(meta)
        .execute(&mut *tx)
        .await;
        let _ = tx.commit().await;
    }
}

#[derive(Deserialize)]
struct Credentials {
    email: String,
    password: String,
    #[serde(default)]
    name: Option<String>,
}

fn user_json(user: &CurrentUser) -> serde_json::Value {
    json!({"id": user.id, "email": user.email, "name": user.name})
}

async fn finish_login_async(
    state: &AppState,
    user: &CurrentUser,
    headers: &HeaderMap,
) -> Result<Response, Response> {
    let ua = headers
        .get("user-agent")
        .and_then(|v| v.to_str().ok())
        .map(String::from);
    let ttl = Duration::seconds(state.config.session_ttl_secs);
    let (token, _expires) = create_session(&state.pool, user.id, ua.as_deref(), ttl)
        .await
        .map_err(map_auth)?;
    let cookie = session_cookie(
        &token,
        state.config.session_ttl_secs,
        state.config.cookie_secure,
    );
    let body = Json(json!({"user": user_json(user)}));
    let mut resp = (StatusCode::OK, body).into_response();
    resp.headers_mut()
        .insert("set-cookie", cookie.parse().expect("cookie header"));
    Ok(resp)
}

/// Fixed-window auth limits: 10 attempts per hour per client identity.
const AUTH_RATE_LIMIT: u64 = 10;
const AUTH_RATE_WINDOW_SECS: u64 = 3600;

fn rate_limited(retry_after_secs: u64) -> Response {
    let mut resp = (
        StatusCode::TOO_MANY_REQUESTS,
        Json(json!({"error": "too many attempts"})),
    )
        .into_response();
    if let Ok(value) = HeaderValue::from_str(&retry_after_secs.to_string()) {
        resp.headers_mut().insert("retry-after", value);
    }
    resp
}

/// The client identity for rate limiting: forwarded address when behind the
/// deployment proxy, else the remote address header value, else email. This
/// is an abuse control, not an authentication signal.
fn rate_key(headers: &HeaderMap, email: &str) -> String {
    let client = headers
        .get("x-forwarded-for")
        .and_then(|v| v.to_str().ok())
        .and_then(|v| v.split(',').next())
        .map(|v| v.trim().to_string())
        .or_else(|| {
            headers
                .get("x-real-ip")
                .and_then(|v| v.to_str().ok())
                .map(|v| v.to_string())
        })
        .unwrap_or_else(|| email.to_lowercase());
    format!("auth:{}", client)
}

async fn enforce_rate_limit(
    state: &AppState,
    headers: &HeaderMap,
    email: &str,
) -> Result<(), Response> {
    match state
        .rate_limiter
        .check(
            &rate_key(headers, email),
            AUTH_RATE_LIMIT,
            AUTH_RATE_WINDOW_SECS,
        )
        .await
    {
        RateDecision::Allow { .. } => Ok(()),
        RateDecision::Deny { retry_after_secs } => Err(rate_limited(retry_after_secs)),
    }
}

/// Local development login and first-user registration. Registration is gated
/// by configuration; it is never open on a production deployment.
async fn register(
    State(state): State<AppState>,
    headers: HeaderMap,
    Json(input): Json<Credentials>,
) -> Result<Response, Response> {
    enforce_rate_limit(&state, &headers, &input.email).await?;
    if !state.config.dev_login_enabled {
        return Err(error_response(&Error::Forbidden {
            action: "register".into(),
            reason: "registration is disabled".into(),
        }));
    }
    if !input.email.contains('@') || input.password.len() < 8 {
        return Err(error_response(&Error::Invalid {
            message: "valid email and password of at least 8 characters required".into(),
        }));
    }
    let password_hash = hash_password(input.password.clone())
        .await
        .map_err(map_auth)?;
    let name = input
        .name
        .clone()
        .unwrap_or_else(|| input.email.split('@').next().unwrap_or("user").to_string());
    let id = Uuid::new_v4();
    let inserted = sqlx::query(
        "INSERT INTO users (id, email, name, password_hash) VALUES ($1, $2, $3, $4)
         ON CONFLICT (email) DO NOTHING RETURNING id",
    )
    .bind(id)
    .bind(input.email.to_lowercase())
    .bind(name.clone())
    .bind(password_hash)
    .fetch_optional(&state.pool)
    .await
    .map_err(|_| error_response(&Error::Internal("database error".into())))?;
    if inserted.is_none() {
        return Err(error_response(&Error::Conflict {
            message: "email already registered".into(),
        }));
    }
    let user = CurrentUser {
        id,
        email: input.email.to_lowercase(),
        name,
        session_id: Uuid::nil(),
    };
    finish_login_async(&state, &user, &headers).await
}

async fn login(
    State(state): State<AppState>,
    headers: HeaderMap,
    Json(input): Json<Credentials>,
) -> Result<Response, Response> {
    enforce_rate_limit(&state, &headers, &input.email).await?;
    let row: Option<(Uuid, String, String, Option<String>)> =
        sqlx::query_as("SELECT id, email, name, password_hash FROM users WHERE email = $1")
            .bind(input.email.to_lowercase())
            .fetch_optional(&state.pool)
            .await
            .map_err(|_| error_response(&Error::Internal("database error".into())))?;
    let (id, email, name, password_hash) =
        row.ok_or_else(|| map_auth(AuthError::InvalidCredentials))?;
    let stored = password_hash.ok_or_else(|| map_auth(AuthError::InvalidCredentials))?;
    let password_ok = verify_password(input.password.clone(), stored)
        .await
        .map_err(map_auth)?;
    if !password_ok {
        return Err(map_auth(AuthError::InvalidCredentials));
    }
    let user = CurrentUser {
        id,
        email,
        name,
        session_id: Uuid::nil(),
    };
    finish_login_async(&state, &user, &headers).await
}

async fn logout(State(state): State<AppState>, headers: HeaderMap) -> Result<Response, Response> {
    let token = token_from_headers(&headers).ok_or_else(|| map_auth(AuthError::NoSession))?;
    let (_user, session_id) = user_from_token(&state.pool, &token)
        .await
        .map_err(map_auth)?;
    revoke_session(&state.pool, session_id)
        .await
        .map_err(map_auth)?;
    let cookie = session_cookie("", 0, state.config.cookie_secure);
    let mut resp = (StatusCode::OK, Json(json!({"ok": true}))).into_response();
    resp.headers_mut()
        .insert("set-cookie", cookie.parse().expect("cookie header"));
    Ok(resp)
}

#[derive(Serialize)]
struct MeResponse {
    user: serde_json::Value,
    companies: Vec<serde_json::Value>,
}

async fn me(State(state): State<AppState>, headers: HeaderMap) -> Result<Response, Response> {
    let user = current_user(&state, &headers).await?;
    let rows =
        sqlx::query("SELECT company_id, role FROM businex.my_memberships($1) ORDER BY company_id")
            .bind(user.id)
            .fetch_all(&state.pool)
            .await
            .map_err(|_| error_response(&Error::Internal("database error".into())))?;
    let companies: Vec<serde_json::Value> = rows
        .iter()
        .map(|r| {
            json!({
                "id": r.get::<Uuid, _>("company_id"),
                "role": r.get::<String, _>("role"),
            })
        })
        .collect();
    Ok(Json(MeResponse {
        user: user_json(&user),
        companies,
    })
    .into_response())
}

#[derive(Deserialize)]
struct NewCompany {
    name: String,
    #[serde(default)]
    slug: Option<String>,
}

async fn create_company(
    State(state): State<AppState>,
    headers: HeaderMap,
    Json(input): Json<NewCompany>,
) -> Result<Response, Response> {
    let user = current_user(&state, &headers).await?;
    if input.name.trim().is_empty() {
        return Err(error_response(&Error::Invalid {
            message: "company name required".into(),
        }));
    }
    let slug = input
        .slug
        .unwrap_or_else(|| slugify(&input.name))
        .to_lowercase();
    let company_id: Uuid = sqlx::query_scalar("SELECT businex.create_company($1, $2, $3)")
        .bind(input.name.trim())
        .bind(&slug)
        .bind(user.id)
        .fetch_one(&state.pool)
        .await
        .map_err(|e| match &e {
            sqlx::Error::Database(db) if db.constraint().is_some() => {
                error_response(&Error::Conflict {
                    message: "company slug already exists".into(),
                })
            }
            _ => error_response(&Error::Internal("database error".into())),
        })?;
    Ok(Json(json!({"id": company_id, "name": input.name, "slug": slug})).into_response())
}

fn slugify(name: &str) -> String {
    let mut out = String::new();
    for c in name.to_lowercase().chars() {
        if c.is_ascii_alphanumeric() {
            out.push(c);
        } else if !out.ends_with('-') && !out.is_empty() {
            out.push('-');
        }
    }
    out.trim_matches('-').to_string()
}

async fn my_companies(
    State(state): State<AppState>,
    headers: HeaderMap,
) -> Result<Response, Response> {
    me(State(state), headers).await
}

#[derive(Serialize)]
struct MemberRow {
    user_id: Uuid,
    email: String,
    name: String,
    role: String,
}

async fn list_members(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(company_id): Path<Uuid>,
) -> Result<Response, Response> {
    let user = current_user(&state, &headers).await?;
    // Membership visibility is a records-read concern of the company itself.
    authorize(&state, &user, company_id, Permission::RecordsRead, "*").await?;
    // Full member listing runs inside the company transaction where RLS
    // confines visibility to the tenant.
    let ctx = businex_db::CompanyContext::new(company_id);
    let mut tx = businex_db::begin_company_tx(&state.pool, ctx)
        .await
        .map_err(|_| error_response(&Error::Internal("database error".into())))?;
    let members = sqlx::query(
        "SELECT m.user_id, u.email, u.name, m.role
         FROM memberships m JOIN users u ON u.id = m.user_id
         WHERE m.company_id = $1
         ORDER BY m.created_at",
    )
    .bind(company_id)
    .fetch_all(&mut *tx)
    .await
    .map_err(|_| error_response(&Error::Internal("database error".into())))?;
    tx.commit()
        .await
        .map_err(|_| error_response(&Error::Internal("database error".into())))?;
    let out: Vec<MemberRow> = members
        .iter()
        .map(|r| MemberRow {
            user_id: r.get("user_id"),
            email: r.get("email"),
            name: r.get("name"),
            role: r.get("role"),
        })
        .collect();
    Ok(Json(json!({"members": out})).into_response())
}

#[derive(Deserialize)]
struct NewInvitation {
    email: String,
    role: String,
}

async fn create_invitation(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(company_id): Path<Uuid>,
    Json(input): Json<NewInvitation>,
) -> Result<Response, Response> {
    let user = current_user(&state, &headers).await?;
    let auth = authorize(&state, &user, company_id, Permission::MembersManage, "*").await?;
    let role = Role::parse(&input.role).ok_or_else(|| {
        error_response(&Error::Invalid {
            message: "unknown role".into(),
        })
    })?;
    if role == Role::Owner {
        return Err(error_response(&Error::Invalid {
            message: "owner role is assigned by transfer only".into(),
        }));
    }
    if !input.email.contains('@') {
        return Err(error_response(&Error::Invalid {
            message: "valid email required".into(),
        }));
    }
    let token = hex::encode(rand::random::<[u8; 32]>());
    let token_hash = hex::encode(Sha256::digest(token.as_bytes()));
    let ctx = businex_db::CompanyContext::with_actor(company_id, user.id);
    let mut tx = businex_db::begin_company_tx(&state.pool, ctx)
        .await
        .map_err(|_| error_response(&Error::Internal("database error".into())))?;
    let id = Uuid::new_v4();
    sqlx::query(
        "INSERT INTO invitations (id, company_id, email, role, token_hash, created_by, expires_at)
         VALUES ($1, $2, $3, $4, $5, $6, $7)",
    )
    .bind(id)
    .bind(company_id)
    .bind(input.email.to_lowercase())
    .bind(role.as_str())
    .bind(token_hash)
    .bind(user.id)
    .bind(Utc::now() + Duration::days(14))
    .execute(&mut *tx)
    .await
    .map_err(|_| error_response(&Error::Internal("database error".into())))?;
    tx.commit()
        .await
        .map_err(|_| error_response(&Error::Internal("database error".into())))?;
    audit(
        &state,
        &auth,
        "member.invite",
        "invitation",
        Some(id),
        json!({"email": input.email, "role": role.as_str()}),
    )
    .await;
    // The raw token is returned once here; in production it is delivered by
    // email and never stored in plain text.
    Ok(
        Json(json!({"id": id, "token": token, "expiresAt": Utc::now() + Duration::days(14)}))
            .into_response(),
    )
}

#[derive(Deserialize)]
struct AcceptInvitation {
    token: String,
}

async fn accept_invitation(
    State(state): State<AppState>,
    headers: HeaderMap,
    Json(input): Json<AcceptInvitation>,
) -> Result<Response, Response> {
    let user = current_user(&state, &headers).await?;
    let token_hash = hex::encode(Sha256::digest(input.token.as_bytes()));
    let row: Option<(Uuid, Uuid, String, String, chrono::DateTime<Utc>, Option<chrono::DateTime<Utc>>)> =
        sqlx::query_as(
            "SELECT id, company_id, email, role, expires_at, accepted_at FROM businex.invitation_by_token($1)",
        )
        .bind(&token_hash)
        .fetch_optional(&state.pool)
        .await
        .map_err(|_| error_response(&Error::Internal("database error".into())))?;
    let (invite_id, company_id, email, role_str, expires_at, accepted_at) =
        row.ok_or_else(|| {
            error_response(&Error::NotFound {
                entity: "invitation".into(),
            })
        })?;
    if accepted_at.is_some() {
        return Err(error_response(&Error::Conflict {
            message: "invitation already used".into(),
        }));
    }
    if expires_at <= Utc::now() {
        return Err(error_response(&Error::Invalid {
            message: "invitation expired".into(),
        }));
    }
    if email != user.email {
        return Err(error_response(&Error::Forbidden {
            action: "invitation.accept".into(),
            reason: "invitation issued to a different email".into(),
        }));
    }
    let role = Role::parse(&role_str)
        .ok_or_else(|| error_response(&Error::Internal("invitation has unknown role".into())))?;

    // Grant membership inside the company transaction so RLS confirms the
    // tenant, then mark the invitation used.
    let ctx = businex_db::CompanyContext::with_actor(company_id, user.id);
    let mut tx = businex_db::begin_company_tx(&state.pool, ctx)
        .await
        .map_err(|_| error_response(&Error::Internal("database error".into())))?;
    let existing: Option<(Uuid,)> =
        sqlx::query_as("SELECT id FROM memberships WHERE company_id = $1 AND user_id = $2")
            .bind(company_id)
            .bind(user.id)
            .fetch_optional(&mut *tx)
            .await
            .map_err(|_| error_response(&Error::Internal("database error".into())))?;
    if existing.is_some() {
        return Err(error_response(&Error::Conflict {
            message: "already a member".into(),
        }));
    }
    sqlx::query("INSERT INTO memberships (id, company_id, user_id, role) VALUES ($1, $2, $3, $4)")
        .bind(Uuid::new_v4())
        .bind(company_id)
        .bind(user.id)
        .bind(role.as_str())
        .execute(&mut *tx)
        .await
        .map_err(|_| error_response(&Error::Internal("database error".into())))?;
    tx.commit()
        .await
        .map_err(|_| error_response(&Error::Internal("database error".into())))?;

    sqlx::query("UPDATE invitations SET accepted_at = now() WHERE id = $1")
        .bind(invite_id)
        .execute(&state.pool)
        .await
        .map_err(|_| error_response(&Error::Internal("database error".into())))?;

    Ok(Json(json!({"companyId": company_id, "role": role.as_str()})).into_response())
}

// ---------------------------------------------------------------------------
// Membership management: role changes and removal, escalation-proof.
// ---------------------------------------------------------------------------

#[derive(Deserialize)]
struct MemberUpdate {
    role: String,
}

/// Change a member's role. Rules enforced here:
/// - requires members.manage (admin or owner);
/// - the owner role is transfer-only and can never be set here;
/// - nobody may grant a role above their own (no self-escalation path);
/// - changing your own role cannot raise it;
/// - every change is audited.
async fn update_member(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path((company_id, user_id)): Path<(Uuid, Uuid)>,
    Json(input): Json<MemberUpdate>,
) -> Result<Response, Response> {
    let user = current_user(&state, &headers).await?;
    let auth = authorize(&state, &user, company_id, Permission::MembersManage, "*").await?;
    let new_role = Role::parse(&input.role).ok_or_else(|| {
        error_response(&Error::Invalid {
            message: "unknown role".into(),
        })
    })?;
    if new_role == Role::Owner {
        return Err(error_response(&Error::Invalid {
            message: "owner role is assigned by transfer only".into(),
        }));
    }
    // No privilege escalation: the target role cannot exceed the actor's role.
    if new_role > auth.role {
        return Err(error_response(&Error::Forbidden {
            action: "members.update".into(),
            reason: "cannot grant a role above your own".into(),
        }));
    }
    let ctx = businex_db::CompanyContext::with_actor(company_id, user.id);
    let mut tx = businex_db::begin_company_tx(&state.pool, ctx)
        .await
        .map_err(|_| error_response(&Error::Internal("database error".into())))?;
    // Serialize owner-affecting membership changes per company so concurrent
    // requests cannot both demote the last two owners.
    sqlx::query("SELECT pg_advisory_xact_lock(hashtextextended($1::text, 0))")
        .bind(company_id.to_string())
        .execute(&mut *tx)
        .await
        .map_err(|_| error_response(&Error::Internal("database error".into())))?;
    let previous: Option<(String,)> =
        sqlx::query_as("SELECT role FROM memberships WHERE company_id = $1 AND user_id = $2")
            .bind(company_id)
            .bind(user_id)
            .fetch_optional(&mut *tx)
            .await
            .map_err(|_| error_response(&Error::Internal("database error".into())))?;
    let (previous_role,) = previous.ok_or_else(|| {
        error_response(&Error::NotFound {
            entity: "membership".into(),
        })
    })?;
    // Fail closed on malformed stored roles: a broken role record must never
    // be silently treated as the lowest privilege.
    let previous_parsed = Role::parse(&previous_role).ok_or_else(|| {
        error_response(&Error::Forbidden {
            action: "members.update".into(),
            reason: "membership role is invalid; manual repair required".into(),
        })
    })?;
    // Never modify a membership whose current role is above your own.
    if previous_parsed > auth.role {
        return Err(error_response(&Error::Forbidden {
            action: "members.update".into(),
            reason: "cannot modify a role above your own".into(),
        }));
    }
    // Owner rows follow the transfer policy: only owners change them.
    if previous_parsed == Role::Owner && auth.role != Role::Owner {
        return Err(error_response(&Error::Forbidden {
            action: "members.update".into(),
            reason: "owner roles change only under the owner transfer policy".into(),
        }));
    }
    if user.id == user_id && new_role > previous_parsed {
        return Err(error_response(&Error::Forbidden {
            action: "members.update".into(),
            reason: "cannot raise your own role".into(),
        }));
    }
    // Demoting an owner must leave at least one owner behind.
    if previous_parsed == Role::Owner && new_role != Role::Owner {
        let owners: (i64,) = sqlx::query_as(
            "SELECT count(*) FROM memberships WHERE company_id = $1 AND role = 'owner'",
        )
        .bind(company_id)
        .fetch_one(&mut *tx)
        .await
        .map_err(|_| error_response(&Error::Internal("database error".into())))?;
        if owners.0 <= 1 {
            return Err(error_response(&Error::Forbidden {
                action: "members.update".into(),
                reason: "the last owner cannot be demoted; transfer ownership first".into(),
            }));
        }
    }
    sqlx::query(
        "UPDATE memberships SET role = $3, updated_at = now()
         WHERE company_id = $1 AND user_id = $2",
    )
    .bind(company_id)
    .bind(user_id)
    .bind(new_role.as_str())
    .execute(&mut *tx)
    .await
    .map_err(|_| error_response(&Error::Internal("database error".into())))?;
    tx.commit()
        .await
        .map_err(|_| error_response(&Error::Internal("database error".into())))?;
    audit(
        &state,
        &auth,
        "member.role_change",
        "membership",
        Some(user_id),
        json!({"from": previous_role, "to": new_role.as_str()}),
    )
    .await;
    Ok(Json(json!({"userId": user_id, "role": new_role.as_str()})).into_response())
}

/// Remove a member. Requires members.manage; the last owner cannot be
/// removed (company transfer is a separate owner-only flow); audited.
async fn remove_member(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path((company_id, user_id)): Path<(Uuid, Uuid)>,
) -> Result<Response, Response> {
    let user = current_user(&state, &headers).await?;
    let auth = authorize(&state, &user, company_id, Permission::MembersManage, "*").await?;
    let ctx = businex_db::CompanyContext::with_actor(company_id, user.id);
    let mut tx = businex_db::begin_company_tx(&state.pool, ctx)
        .await
        .map_err(|_| error_response(&Error::Internal("database error".into())))?;
    // Serialize owner-affecting membership changes per company.
    sqlx::query("SELECT pg_advisory_xact_lock(hashtextextended($1::text, 0))")
        .bind(company_id.to_string())
        .execute(&mut *tx)
        .await
        .map_err(|_| error_response(&Error::Internal("database error".into())))?;
    let target: Option<(String,)> =
        sqlx::query_as("SELECT role FROM memberships WHERE company_id = $1 AND user_id = $2")
            .bind(company_id)
            .bind(user_id)
            .fetch_optional(&mut *tx)
            .await
            .map_err(|_| error_response(&Error::Internal("database error".into())))?;
    let (target_role,) = target.ok_or_else(|| {
        error_response(&Error::NotFound {
            entity: "membership".into(),
        })
    })?;
    // Fail closed on malformed stored roles.
    let target_parsed = Role::parse(&target_role).ok_or_else(|| {
        error_response(&Error::Forbidden {
            action: "members.remove".into(),
            reason: "membership role is invalid; manual repair required".into(),
        })
    })?;
    // Never remove a membership whose role is above your own.
    if target_parsed > auth.role {
        return Err(error_response(&Error::Forbidden {
            action: "members.remove".into(),
            reason: "cannot remove a role above your own".into(),
        }));
    }
    if target_parsed == Role::Owner {
        if auth.role != Role::Owner {
            return Err(error_response(&Error::Forbidden {
                action: "members.remove".into(),
                reason: "owner roles change only under the owner transfer policy".into(),
            }));
        }
        let owners: (i64,) = sqlx::query_as(
            "SELECT count(*) FROM memberships WHERE company_id = $1 AND role = 'owner'",
        )
        .bind(company_id)
        .fetch_one(&mut *tx)
        .await
        .map_err(|_| error_response(&Error::Internal("database error".into())))?;
        if owners.0 <= 1 {
            return Err(error_response(&Error::Forbidden {
                action: "members.remove".into(),
                reason: "the last owner cannot be removed; transfer ownership first".into(),
            }));
        }
    }
    sqlx::query("DELETE FROM memberships WHERE company_id = $1 AND user_id = $2")
        .bind(company_id)
        .bind(user_id)
        .execute(&mut *tx)
        .await
        .map_err(|_| error_response(&Error::Internal("database error".into())))?;
    tx.commit()
        .await
        .map_err(|_| error_response(&Error::Internal("database error".into())))?;
    audit(
        &state,
        &auth,
        "member.remove",
        "membership",
        Some(user_id),
        json!({"role": target_role}),
    )
    .await;
    Ok(Json(json!({"removed": user_id})).into_response())
}

// ---------------------------------------------------------------------------
// Agent / generated-app action grants: explicit permission-resource pairs.
// ---------------------------------------------------------------------------

#[derive(Deserialize)]
struct NewGrant {
    actor_type: String,
    actor_id: Uuid,
    permission: String,
    resource: String,
    #[serde(default)]
    expires_in_secs: Option<i64>,
}

/// Create a scoped action grant for an agent or generated app. The grant is
/// exactly one permission on one resource: reading never implies writing, and
/// the grant cannot exceed the granting member's own role permissions.
async fn create_grant(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(company_id): Path<Uuid>,
    Json(input): Json<NewGrant>,
) -> Result<Response, Response> {
    let user = current_user(&state, &headers).await?;
    let auth = authorize(&state, &user, company_id, Permission::AgentsManage, "*").await?;
    let actor_type = match input.actor_type.as_str() {
        "agent" => "agent",
        "generated_app" => "generated_app",
        _ => {
            return Err(error_response(&Error::Invalid {
                message: "actor_type must be agent or generated_app".into(),
            }))
        }
    };
    let permission = Permission::parse(&input.permission).map_err(|e| error_response(&e))?;
    if input.resource.trim().is_empty() {
        return Err(error_response(&Error::Invalid {
            message: "resource is required".into(),
        }));
    }
    // The issuer cannot hand out permissions they do not hold themselves.
    if !auth.role.allows(permission) {
        return Err(error_response(&Error::Forbidden {
            action: "grants.create".into(),
            reason: "cannot grant a permission your role does not hold".into(),
        }));
    }
    // Bound the expiry window: chrono would panic on absurd i64 values and
    // open-ended grants are not a sensible default.
    let expires_at = match input.expires_in_secs {
        Some(secs) if !(60..=31_536_000).contains(&secs) => {
            return Err(error_response(&Error::Invalid {
                message: "expires_in_secs must be between 60 and 31536000".into(),
            }));
        }
        Some(secs) => Some(Utc::now() + Duration::seconds(secs)),
        None => None,
    };
    let ctx = businex_db::CompanyContext::with_actor(company_id, user.id);
    let mut tx = businex_db::begin_company_tx(&state.pool, ctx)
        .await
        .map_err(|_| error_response(&Error::Internal("database error".into())))?;
    let grant_id = Uuid::new_v4();
    sqlx::query(
        "INSERT INTO action_grants
           (id, company_id, actor_type, actor_id, permission, resource, granted_by, expires_at)
         VALUES ($1, $2, $3, $4, $5, $6, $7, $8)",
    )
    .bind(grant_id)
    .bind(company_id)
    .bind(actor_type)
    .bind(input.actor_id)
    .bind(permission.as_str())
    .bind(input.resource.trim())
    .bind(user.id)
    .bind(expires_at)
    .execute(&mut *tx)
    .await
    .map_err(|_| error_response(&Error::Internal("database error".into())))?;
    tx.commit()
        .await
        .map_err(|_| error_response(&Error::Internal("database error".into())))?;
    audit(
        &state,
        &auth,
        "grant.create",
        "action_grant",
        Some(grant_id),
        json!({
            "actorType": actor_type,
            "actorId": input.actor_id,
            "permission": permission.as_str(),
            "resource": input.resource.trim(),
        }),
    )
    .await;
    Ok(Json(json!({
        "id": grant_id,
        "permission": permission.as_str(),
        "resource": input.resource.trim(),
        "expiresAt": expires_at,
    }))
    .into_response())
}

async fn list_grants(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(company_id): Path<Uuid>,
) -> Result<Response, Response> {
    let user = current_user(&state, &headers).await?;
    authorize(&state, &user, company_id, Permission::AgentsManage, "*").await?;
    let ctx = businex_db::CompanyContext::new(company_id);
    let mut tx = businex_db::begin_company_tx(&state.pool, ctx)
        .await
        .map_err(|_| error_response(&Error::Internal("database error".into())))?;
    let rows = sqlx::query(
        "SELECT id, actor_type, actor_id, permission, resource, expires_at
         FROM action_grants WHERE company_id = $1 ORDER BY created_at",
    )
    .bind(company_id)
    .fetch_all(&mut *tx)
    .await
    .map_err(|_| error_response(&Error::Internal("database error".into())))?;
    tx.commit()
        .await
        .map_err(|_| error_response(&Error::Internal("database error".into())))?;
    let grants: Vec<serde_json::Value> = rows
        .iter()
        .map(|r| {
            json!({
                "id": r.get::<Uuid, _>("id"),
                "actorType": r.get::<String, _>("actor_type"),
                "actorId": r.get::<Uuid, _>("actor_id"),
                "permission": r.get::<String, _>("permission"),
                "resource": r.get::<String, _>("resource"),
                "expiresAt": r.get::<Option<chrono::DateTime<Utc>>, _>("expires_at"),
            })
        })
        .collect();
    Ok(Json(json!({"grants": grants})).into_response())
}

/// Revoke one grant. Revocation is immediate and audited; the grant row is
/// retained with a revoked marker only through the audit log (delete here).
async fn revoke_grant(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path((company_id, grant_id)): Path<(Uuid, Uuid)>,
) -> Result<Response, Response> {
    let user = current_user(&state, &headers).await?;
    let auth = authorize(&state, &user, company_id, Permission::AgentsManage, "*").await?;
    let ctx = businex_db::CompanyContext::with_actor(company_id, user.id);
    let mut tx = businex_db::begin_company_tx(&state.pool, ctx)
        .await
        .map_err(|_| error_response(&Error::Internal("database error".into())))?;
    let deleted = sqlx::query("DELETE FROM action_grants WHERE company_id = $1 AND id = $2")
        .bind(company_id)
        .bind(grant_id)
        .execute(&mut *tx)
        .await
        .map_err(|_| error_response(&Error::Internal("database error".into())))?;
    if deleted.rows_affected() == 0 {
        return Err(error_response(&Error::NotFound {
            entity: "grant".into(),
        }));
    }
    tx.commit()
        .await
        .map_err(|_| error_response(&Error::Internal("database error".into())))?;
    audit(
        &state,
        &auth,
        "grant.revoke",
        "action_grant",
        Some(grant_id),
        json!({}),
    )
    .await;
    Ok(Json(json!({"revoked": grant_id})).into_response())
}
