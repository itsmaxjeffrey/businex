//! HTTP API surface: Axum + Tower.
//!
//! This milestone provides the operational surface (liveness, readiness,
//! health with dependency status) and the middleware stack (request ids,
//! tracing, compression). Authenticated routes arrive in Phase 2 on top of the
//! same AppState.

use axum::extract::State;
use axum::http::{HeaderValue, StatusCode};
use axum::response::{IntoResponse, Response};
use axum::routing::get;
use axum::{Json, Router};
use businex_events::{Relay, Status};
use serde_json::json;
use std::time::Instant;
use tower_http::request_id::{MakeRequestUuid, PropagateRequestIdLayer, SetRequestIdLayer};
use tower_http::trace::TraceLayer;

pub mod auth;
pub mod oidc;
pub mod ratelimit;
pub mod routes_apps;
pub mod routes_identity;

pub const VERSION: &str = env!("CARGO_PKG_VERSION");

/// Runtime configuration. Session cookies are HttpOnly + SameSite=Strict and
/// Secure outside explicit local development.
#[derive(Debug, Clone)]
pub struct AppConfig {
    pub session_ttl_secs: i64,
    pub cookie_secure: bool,
    pub dev_login_enabled: bool,
}

impl Default for AppConfig {
    fn default() -> Self {
        AppConfig {
            session_ttl_secs: 86_400,
            cookie_secure: true,
            dev_login_enabled: false,
        }
    }
}

#[derive(Clone)]
pub struct AppState {
    pub pool: sqlx::PgPool,
    pub relay: Relay,
    pub started_at: Instant,
    pub config: AppConfig,
    pub rate_limiter: std::sync::Arc<dyn ratelimit::RateLimiter>,
    pub oidc: Option<oidc::OidcRuntime>,
}

impl AppState {
    /// In-process limiter default for tests and single-node development.
    pub fn with_memory_limiter(pool: sqlx::PgPool, relay: Relay, config: AppConfig) -> Self {
        AppState {
            pool,
            relay,
            started_at: Instant::now(),
            config,
            rate_limiter: std::sync::Arc::new(ratelimit::MemoryRateLimiter::new()),
            oidc: None,
        }
    }
}

pub fn router(state: AppState) -> Router {
    let x_request_id = axum::http::HeaderName::from_static("x-request-id");
    Router::new()
        .route("/healthz", get(healthz))
        .route("/readyz", get(readyz))
        .route("/api/health", get(api_health))
        .merge(routes_identity::router())
        .merge(routes_apps::router())
        .merge(oidc::router())
        .layer(TraceLayer::new_for_http())
        .layer(PropagateRequestIdLayer::new(x_request_id.clone()))
        .layer(SetRequestIdLayer::new(x_request_id, MakeRequestUuid))
        .with_state(state)
}

/// Liveness: the process is up. No dependency checks here on purpose, so a
/// dependency outage does not cause restart loops.
async fn healthz() -> Response {
    (StatusCode::OK, "ok").into_response()
}

/// Readiness: dependencies are actually usable right now.
async fn readyz(State(state): State<AppState>) -> Response {
    let db_ok = businex_db::ping(&state.pool).await.is_ok();
    let redis = redis_label(state.relay.status());
    if !db_ok {
        return (
            StatusCode::SERVICE_UNAVAILABLE,
            Json(json!({"status": "not_ready", "db": "down", "redis": redis, "version": VERSION})),
        )
            .into_response();
    }
    (
        StatusCode::OK,
        Json(json!({"status": "ready", "db": "up", "redis": redis, "version": VERSION})),
    )
        .into_response()
}

/// Health summary kept compatible with the existing deployment checks.
async fn api_health(State(state): State<AppState>) -> Response {
    let db_ok = businex_db::ping(&state.pool).await.is_ok();
    let redis = redis_label(state.relay.status());
    let status = if db_ok { "ok" } else { "degraded" };
    let code = if db_ok {
        StatusCode::OK
    } else {
        StatusCode::SERVICE_UNAVAILABLE
    };
    (
        code,
        Json(json!({
            "status": status,
            "version": VERSION,
            "db": if db_ok { "connected" } else { "down" },
            "redis": redis,
            "uptimeSeconds": state.started_at.elapsed().as_secs(),
        })),
    )
        .into_response()
}

fn redis_label(status: Status) -> &'static str {
    match status {
        Status::Connected => "connected",
        Status::Degraded => "degraded",
        Status::Disabled => "disabled",
    }
}

/// Initialize structured JSON logging. The logging policy is: never log
/// secrets, tokens, passwords, raw provider errors or full request headers.
pub fn init_tracing(default_level: &str) {
    use tracing_subscriber::EnvFilter;
    let filter =
        EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new(default_level));
    tracing_subscriber::fmt()
        .with_env_filter(filter)
        .json()
        .with_current_span(false)
        .flatten_event(true)
        .init();
}

/// Build a response from the shared error type without leaking internals.
pub fn error_response(err: &businex_core::Error) -> Response {
    let (code, message) = match err {
        businex_core::Error::NotFound { entity } => (StatusCode::NOT_FOUND, entity.clone()),
        businex_core::Error::Forbidden { action, reason } => {
            (StatusCode::FORBIDDEN, format!("{}: {}", action, reason))
        }
        businex_core::Error::Conflict { message } => (StatusCode::CONFLICT, message.clone()),
        businex_core::Error::Invalid { message } => (StatusCode::BAD_REQUEST, message.clone()),
        businex_core::Error::Internal(_) => {
            (StatusCode::INTERNAL_SERVER_ERROR, "internal error".into())
        }
    };
    let mut resp = (code, Json(json!({"error": message}))).into_response();
    if let Ok(value) = HeaderValue::from_str("application/json") {
        resp.headers_mut()
            .insert(axum::http::header::CONTENT_TYPE, value);
    }
    resp
}
