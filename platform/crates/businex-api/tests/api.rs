//! API surface tests. Uses a real PostgreSQL for readiness checks.

use axum::body::Body;
use axum::http::{Request, StatusCode};
use businex_api::{router, AppConfig, AppState};
use businex_db::TestDb;
use businex_events::{Relay, RelayOptions};
use serde_json::Value;
use sqlx::postgres::PgPoolOptions;
use std::time::Instant;
use tower::ServiceExt;

/// A disposable database per test (requires BUSINEX_TEST_DATABASE_URL).
async fn test_state() -> (TestDb, AppState) {
    let db = TestDb::new().await;
    let state = AppState {
        pool: db.pool.clone(),
        relay: Relay::new(RelayOptions::default()),
        started_at: Instant::now(),
        config: AppConfig::default(),
    };
    (db, state)
}

async fn body_json(resp: axum::response::Response) -> Value {
    let bytes = axum::body::to_bytes(resp.into_body(), usize::MAX)
        .await
        .expect("body");
    serde_json::from_slice(&bytes).expect("json body")
}

#[tokio::test]
async fn healthz_is_live() {
    let (_db, state) = test_state().await;
    let app = router(state);
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
    let (_db, state) = test_state().await;
    let app = router(state);
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
        config: AppConfig::default(),
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
    let (_db, state) = test_state().await;
    let app = router(state);
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
    let (_db, state) = test_state().await;
    let app = router(state);
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
