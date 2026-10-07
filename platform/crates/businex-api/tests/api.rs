//! API surface tests. Uses a real PostgreSQL for readiness checks.

use axum::body::Body;
use axum::http::{Request, StatusCode};
use businex_api::{router, AppState};
use businex_events::{Relay, RelayOptions};
use serde_json::Value;
use sqlx::postgres::PgPoolOptions;
use std::sync::OnceLock;
use std::time::Instant;
use tower::ServiceExt;

fn database_url() -> String {
    std::env::var("BUSINEX_TEST_DATABASE_URL")
        .or_else(|_| std::env::var("DATABASE_URL"))
        .expect("set BUSINEX_TEST_DATABASE_URL to a disposable PostgreSQL database")
}

async fn test_state() -> AppState {
    static MIGRATED: OnceLock<()> = OnceLock::new();
    let pool = businex_db::connect(&database_url(), 5).await.expect("connect");
    if MIGRATED.get().is_none() {
        businex_db::run_migrations(&pool).await.expect("migrate");
        let _ = MIGRATED.set(());
    }
    AppState {
        pool,
        relay: Relay::new(RelayOptions::default()),
        started_at: Instant::now(),
    }
}

async fn body_json(resp: axum::response::Response) -> Value {
    let bytes = axum::body::to_bytes(resp.into_body(), usize::MAX)
        .await
        .expect("body");
    serde_json::from_slice(&bytes).expect("json body")
}

#[tokio::test]
async fn healthz_is_live() {
    let app = router(test_state().await);
    let resp = app
        .oneshot(
            Request::builder()
                .uri("/healthz")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(resp.status(), StatusCode::OK);
}

#[tokio::test]
async fn readyz_reports_dependencies() {
    let app = router(test_state().await);
    let resp = app
        .oneshot(
            Request::builder()
                .uri("/readyz")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(resp.status(), StatusCode::OK);
    let json = body_json(resp).await;
    assert_eq!(json["db"], "up");
    assert_eq!(json["status"], "ready");
}

#[tokio::test]
async fn readyz_fails_when_database_is_down() {
    let bad_pool = PgPoolOptions::new()
        .connect_lazy("postgres://nouser:nopass@127.0.0.1:1/nope")
        .expect("lazy pool");
    let app = router(AppState {
        pool: bad_pool,
        relay: Relay::new(RelayOptions::default()),
        started_at: Instant::now(),
    });
    let resp = app
        .oneshot(
            Request::builder()
                .uri("/readyz")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(resp.status(), StatusCode::SERVICE_UNAVAILABLE);
}

#[tokio::test]
async fn api_health_matches_deployment_checks() {
    let app = router(test_state().await);
    let resp = app
        .oneshot(
            Request::builder()
                .uri("/api/health")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(resp.status(), StatusCode::OK);
    let json = body_json(resp).await;
    assert_eq!(json["status"], "ok");
    assert_eq!(json["db"], "connected");
    assert_eq!(json["redis"], "disabled");
}

#[tokio::test]
async fn every_response_carries_a_request_id() {
    let app = router(test_state().await);
    let resp = app
        .oneshot(
            Request::builder()
                .uri("/healthz")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert!(
        resp.headers().get("x-request-id").is_some(),
        "responses must carry a request id for trace correlation"
    );
}
