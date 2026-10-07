//! Identity and tenancy API tests: real HTTP calls against the router with a
//! real PostgreSQL. Covers session auth, company creation, invitations, role
//! enforcement and cross-tenant denial including member listing.

use axum::body::Body;
use axum::http::{Request, StatusCode};
use businex_api::{router, AppConfig, AppState};
use businex_db::TestDb;
use businex_events::{Relay, RelayOptions};
use serde_json::{json, Value};
use std::time::Instant;
use tower::ServiceExt;

/// A disposable database per test (requires BUSINEX_TEST_DATABASE_URL).
async fn test_state() -> (TestDb, AppState) {
    let db = TestDb::new().await;
    let state = AppState {
        pool: db.pool.clone(),
        relay: Relay::new(RelayOptions::default()),
        started_at: Instant::now(),
        config: AppConfig {
            session_ttl_secs: 3600,
            cookie_secure: false,
            dev_login_enabled: true,
        },
    };
    (db, state)
}

struct Client {
    app: axum::Router,
    cookie: Option<String>,
}

impl Client {
    fn new(app: axum::Router) -> Self {
        Client { app, cookie: None }
    }

    async fn call(&mut self, method: &str, uri: &str, body: Option<Value>) -> (StatusCode, Value, Option<String>) {
        let mut builder = Request::builder()
            .method(method)
            .uri(uri)
            .header("content-type", "application/json");
        if let Some(cookie) = &self.cookie {
            builder = builder.header("cookie", cookie);
        }
        let req = builder
            .body(match body {
                Some(v) => Body::from(v.to_string()),
                None => Body::empty(),
            })
            .expect("request");
        let resp = self.app.clone().oneshot(req).await.expect("response");
        let status = resp.status();
        let set_cookie = resp
            .headers()
            .get("set-cookie")
            .and_then(|v| v.to_str().ok())
            .map(|s| s.to_string());
        if let Some(sc) = &set_cookie {
            // Keep only the pair, drop attributes.
            let pair = sc.split(';').next().unwrap_or("").to_string();
            if pair.contains('=') {
                self.cookie = Some(pair);
            }
        }
        let bytes = axum::body::to_bytes(resp.into_body(), usize::MAX)
            .await
            .expect("body");
        let value = if bytes.is_empty() {
            Value::Null
        } else {
            serde_json::from_slice(&bytes).unwrap_or(Value::Null)
        };
        (status, value, set_cookie)
    }

    async fn get(&mut self, uri: &str) -> (StatusCode, Value) {
        let (s, v, _) = self.call("GET", uri, None).await;
        (s, v)
    }

    async fn post(&mut self, uri: &str, body: Value) -> (StatusCode, Value) {
        let (s, v, _) = self.call("POST", uri, Some(body)).await;
        (s, v)
    }
}

fn email(tag: &str) -> String {
    format!("{}-{}@example.test", tag, uuid::Uuid::new_v4())
}

async fn register(app: axum::Router, tag: &str) -> (Client, String) {
    let mut client = Client::new(app);
    let addr = email(tag);
    let (status, body) = client
        .post(
            "/api/auth/register",
            json!({"email": addr, "password": "correct horse battery", "name": tag}),
        )
        .await;
    assert_eq!(status, StatusCode::OK, "register: {}", body);
    (client, addr)
}

#[tokio::test]
async fn register_login_me_logout_flow() {
    let (_db, state) = test_state().await;
    let (mut client, addr) = register(app_for(&state), "alice").await;

    let (status, me) = client.get("/api/auth/me").await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(me["user"]["email"], addr.as_str());
    assert!(me["companies"].as_array().unwrap().is_empty());

    // Wrong password is rejected without a session.
    let mut other = Client::new(app_for(&state));
    let (status, _) = other
        .post("/api/auth/login", json!({"email": addr, "password": "wrong password here"}))
        .await;
    assert_eq!(status, StatusCode::FORBIDDEN);

    // Correct password logs in and sets a session cookie.
    let (status, body) = other
        .post("/api/auth/login", json!({"email": addr, "password": "correct horse battery"}))
        .await;
    assert_eq!(status, StatusCode::OK, "{}", body);

    // Logout invalidates the session.
    let (status, _) = other.post("/api/auth/logout", json!({})).await;
    assert_eq!(status, StatusCode::OK);
    let (status, _) = other.get("/api/auth/me").await;
    assert_eq!(status, StatusCode::FORBIDDEN, "session must be dead after logout");
}

#[tokio::test]
async fn two_companies_two_roles_isolated() {
    let (_db, state) = test_state().await;
    let (mut owner_a, _) = register(app_for(&state), "owner-a").await;
    let (mut owner_b, _) = register(app_for(&state), "owner-b").await;
    let (mut member_a, member_a_email) = register(app_for(&state), "member-a").await;
    let (mut viewer_a, viewer_a_email) = register(app_for(&state), "viewer-a").await;

    // Each owner creates a company and becomes its owner.
    let (status, co_a) = owner_a
        .post("/api/companies", json!({"name": "Alpha Trading"}))
        .await;
    assert_eq!(status, StatusCode::OK, "{}", co_a);
    let company_a = co_a["id"].as_str().unwrap().to_string();
    let (status, co_b) = owner_b
        .post("/api/companies", json!({"name": "Beta Industries"}))
        .await;
    assert_eq!(status, StatusCode::OK);
    let company_b = co_b["id"].as_str().unwrap().to_string();

    // Owner A invites a member and a viewer; they accept.
    for (client, role) in [(&mut member_a, "member"), (&mut viewer_a, "viewer")] {
        let addr = if role == "member" {
            &member_a_email
        } else {
            &viewer_a_email
        };
        let (status, inv) = owner_a
            .post(
                &format!("/api/companies/{}/invitations", company_a),
                json!({"email": addr, "role": role}),
            )
            .await;
        assert_eq!(status, StatusCode::OK, "{}", inv);
        let token = inv["token"].as_str().unwrap().to_string();
        let (status, accepted) = client
            .post("/api/invitations/accept", json!({"token": token}))
            .await;
        assert_eq!(status, StatusCode::OK, "{}", accepted);
        assert_eq!(accepted["role"], role);
    }

    // Member A can see company A's member list.
    let (status, members) = member_a
        .get(&format!("/api/companies/{}/members", company_a))
        .await;
    assert_eq!(status, StatusCode::OK, "{}", members);
    assert_eq!(members["members"].as_array().unwrap().len(), 3);

    // Cross-tenant denial: company B's owner cannot read company A's members.
    let (status, _) = owner_b
        .get(&format!("/api/companies/{}/members", company_a))
        .await;
    assert_eq!(status, StatusCode::FORBIDDEN, "cross-tenant read must be denied");

    // Privilege denial: viewer cannot manage members (invite).
    let (status, _) = viewer_a
        .post(
            &format!("/api/companies/{}/invitations", company_a),
            json!({"email": email("intruder"), "role": "admin"}),
        )
        .await;
    assert_eq!(status, StatusCode::FORBIDDEN, "viewer must not invite");

    // Privilege denial: member (non-admin) cannot manage members either.
    let (status, _) = member_a
        .post(
            &format!("/api/companies/{}/invitations", company_a),
            json!({"email": email("intruder"), "role": "member"}),
        )
        .await;
    assert_eq!(status, StatusCode::FORBIDDEN, "member must not invite");

    // Cross-tenant denial: owner B cannot invite into company A.
    let (status, _) = owner_b
        .post(
            &format!("/api/companies/{}/invitations", company_a),
            json!({"email": email("intruder"), "role": "admin"}),
        )
        .await;
    assert_eq!(status, StatusCode::FORBIDDEN, "cross-tenant invite must be denied");

    // Owner A cannot claim company B (nonexistent membership path).
    let (status, _) = owner_a
        .get(&format!("/api/companies/{}/members", company_b))
        .await;
    assert_eq!(status, StatusCode::FORBIDDEN);
}

#[tokio::test]
async fn invitation_token_is_single_use_and_bound_to_email() {
    let (_db, state) = test_state().await;
    let (mut owner, _) = register(app_for(&state), "owner").await;
    let (mut invitee, invitee_email) = register(app_for(&state), "invitee").await;
    let (mut stranger, _) = register(app_for(&state), "stranger").await;

    let (status, co) = owner.post("/api/companies", json!({"name": "Gamma Ltd"})).await;
    assert_eq!(status, StatusCode::OK);
    let company = co["id"].as_str().unwrap().to_string();

    let (status, inv) = owner
        .post(
            &format!("/api/companies/{}/invitations", company),
            json!({"email": invitee_email, "role": "member"}),
        )
        .await;
    assert_eq!(status, StatusCode::OK);
    let token = inv["token"].as_str().unwrap().to_string();

    // A different signed-in user cannot redeem someone else's invitation.
    let (status, _) = stranger
        .post("/api/invitations/accept", json!({"token": token.clone()}))
        .await;
    assert_eq!(status, StatusCode::FORBIDDEN, "invitation is bound to its email");

    // The rightful invitee redeems it exactly once.
    let (status, _) = invitee
        .post("/api/invitations/accept", json!({"token": token.clone()}))
        .await;
    assert_eq!(status, StatusCode::OK);
    let (status, _) = invitee
        .post("/api/invitations/accept", json!({"token": token}))
        .await;
    assert_eq!(status, StatusCode::CONFLICT, "invitation must be single use");
}

fn app_for(state: &AppState) -> axum::Router {
    router(state.clone())
}
