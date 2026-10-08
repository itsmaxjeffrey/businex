//! End-to-end OIDC tests against a mock provider over real HTTP with
//! RS256-signed id tokens. The production verifier (signature, issuer,
//! audience, expiry, nonce) must accept these fixtures; browser binding,
//! single-use state and (issuer, subject) mapping are exercised through the
//! real routes against the disposable test database.

use axum::body::Body;
use axum::extract::{Form, State};
use axum::http::{Request, StatusCode};
use axum::response::{IntoResponse, Response};
use axum::routing::{get, post};
use axum::{Json, Router};
use base64::Engine;
use businex_api::oidc::{ClientCred, OidcConfig, OidcRuntime};
use businex_api::{router, AppConfig, AppState};
use businex_db::TestDb;
use businex_events::{Relay, RelayOptions};
use chrono::{Duration, Utc};
use serde_json::{json, Value};
use sha2::{Digest, Sha256};
use std::collections::HashMap;
use std::sync::{Arc, Mutex};
use tower::ServiceExt;

use openidconnect::core::{
    CoreIdToken, CoreIdTokenClaims, CoreJsonWebKeySet, CoreJwsSigningAlgorithm,
    CoreRsaPrivateSigningKey,
};
use openidconnect::{
    Audience, EmptyAdditionalClaims, EndUserEmail, IssuerUrl, JsonWebKeyId, Nonce,
    PrivateSigningKey, StandardClaims, SubjectIdentifier,
};

const CLIENT_ID: &str = "test-client";

/// Per-test knobs for the mock token endpoint.
struct MockSpec {
    nonce: String,
    subject: String,
    email: String,
    email_verified: bool,
    issuer_override: Option<String>,
    aud_override: Option<String>,
    exp_offset_secs: i64,
    wrong_key: bool,
    expected_challenge: Option<String>,
    seen_verifier: Option<String>,
}

impl MockSpec {
    fn new() -> Self {
        MockSpec {
            nonce: String::new(),
            subject: "user-42".to_string(),
            email: "user42@example.test".to_string(),
            email_verified: true,
            issuer_override: None,
            aud_override: None,
            exp_offset_secs: 300,
            wrong_key: false,
            expected_challenge: None,
            seen_verifier: None,
        }
    }
}

#[derive(Clone)]
struct MockState {
    base: String,
    spec: Arc<Mutex<MockSpec>>,
}

fn signing_key(wrong: bool) -> CoreRsaPrivateSigningKey {
    let pem = if wrong {
        include_str!("fixtures/oidc_rsa_wrong.pem")
    } else {
        include_str!("fixtures/oidc_rsa_pkcs1.pem")
    };
    CoreRsaPrivateSigningKey::from_pem(pem, Some(JsonWebKeyId::new("test-key".to_string())))
        .expect("fixture pem")
}

async fn discovery(State(st): State<MockState>) -> Json<Value> {
    let issuer = st.base.clone();
    Json(json!({
        "issuer": issuer,
        "authorization_endpoint": format!("{}/authorize", issuer),
        "token_endpoint": format!("{}/token", issuer),
        "jwks_uri": format!("{}/jwks.json", issuer),
        "response_types_supported": ["code"],
        "subject_types_supported": ["public"],
        "id_token_signing_alg_values_supported": ["RS256"]
    }))
}

async fn jwks() -> Json<Value> {
    let key = signing_key(false);
    let set = CoreJsonWebKeySet::new(vec![key.as_verification_key()]);
    Json(serde_json::to_value(set).expect("jwk json"))
}

async fn token(State(st): State<MockState>, Form(form): Form<HashMap<String, String>>) -> Response {
    let mut spec = st.spec.lock().expect("mock spec");
    if let Some(challenge) = spec.expected_challenge.clone() {
        let verifier = form.get("code_verifier").cloned().unwrap_or_default();
        let digest = Sha256::digest(verifier.as_bytes());
        let encoded = base64::engine::general_purpose::URL_SAFE_NO_PAD.encode(digest);
        spec.seen_verifier = Some(verifier);
        if encoded != challenge {
            return (
                StatusCode::BAD_REQUEST,
                Json(json!({"error": "invalid_grant"})),
            )
                .into_response();
        }
    }
    let issuer = spec
        .issuer_override
        .clone()
        .unwrap_or_else(|| st.base.clone());
    let aud = spec
        .aud_override
        .clone()
        .unwrap_or_else(|| CLIENT_ID.to_string());
    let claims = CoreIdTokenClaims::new(
        IssuerUrl::new(issuer).expect("issuer"),
        vec![Audience::new(aud)],
        Utc::now() + Duration::seconds(spec.exp_offset_secs),
        Utc::now(),
        StandardClaims::new(SubjectIdentifier::new(spec.subject.clone()))
            .set_email(Some(EndUserEmail::new(spec.email.clone())))
            .set_email_verified(Some(spec.email_verified)),
        EmptyAdditionalClaims {},
    )
    .set_nonce(Some(Nonce::new(spec.nonce.clone())));
    let key = signing_key(spec.wrong_key);
    let signed = CoreIdToken::new(
        claims,
        &key,
        CoreJwsSigningAlgorithm::RsaSsaPkcs1V15Sha256,
        None,
        None,
    )
    .expect("sign id token")
    .to_string();
    let mut out = serde_json::Map::new();
    out.insert("access_token".to_string(), json!("mock-access"));
    out.insert("token_type".to_string(), json!("Bearer"));
    out.insert("id_token".to_string(), json!(signed));
    Json(Value::Object(out)).into_response()
}

/// Start the mock provider on a random loopback port.
fn mock_provider() -> (String, Arc<Mutex<MockSpec>>) {
    let listener = std::net::TcpListener::bind("127.0.0.1:0").expect("bind mock");
    listener
        .set_nonblocking(true)
        .expect("mock listener nonblocking");
    let addr = listener.local_addr().expect("mock addr");
    let base = format!("http://127.0.0.1:{}", addr.port());
    let spec = Arc::new(Mutex::new(MockSpec::new()));
    let state = MockState {
        base: base.clone(),
        spec: spec.clone(),
    };
    let app = Router::new()
        .route("/.well-known/openid-configuration", get(discovery))
        .route("/jwks.json", get(jwks))
        .route("/token", post(token))
        .with_state(state);
    let tokio_listener = tokio::net::TcpListener::from_std(listener).expect("tokio listener");
    tokio::spawn(async move {
        axum::serve(tokio_listener, app).await.expect("mock serve");
    });
    (base, spec)
}

/// Minimal cookie-aware oneshot client (one browser = one instance).
struct Client {
    app: Router,
    cookie: Option<String>,
}

impl Client {
    fn new(app: Router) -> Self {
        Client { app, cookie: None }
    }

    async fn call(&mut self, uri: &str) -> (StatusCode, Value, Option<String>) {
        let mut builder = Request::builder()
            .method("GET")
            .uri(uri)
            .header("content-type", "application/json");
        if let Some(cookie) = &self.cookie {
            builder = builder.header("cookie", cookie.clone());
        }
        let req = builder.body(Body::empty()).expect("request");
        let resp = self.app.clone().oneshot(req).await.expect("response");
        let status = resp.status();
        let set_cookie = resp
            .headers()
            .get("set-cookie")
            .and_then(|v| v.to_str().ok())
            .map(|s| s.to_string());
        if let Some(sc) = &set_cookie {
            let pair = sc.split(";").next().unwrap_or("").to_string();
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

    fn set_cookie(&mut self, value: String) {
        self.cookie = Some(value);
    }
}

/// The sandbox host routes through an HTTP proxy that answers 502 for
/// loopback targets; reqwest honors proxy env at client build time, so the
/// mock provider on 127.0.0.1 must be exempted before any client is built.
fn ensure_loopback_no_proxy() {
    static ONCE: std::sync::Once = std::sync::Once::new();
    ONCE.call_once(|| {
        std::env::set_var("NO_PROXY", "127.0.0.1,localhost");
        std::env::set_var("no_proxy", "127.0.0.1,localhost");
    });
}

fn test_config() -> AppConfig {
    AppConfig {
        session_ttl_secs: 3600,
        cookie_secure: false,
        dev_login_enabled: true,
    }
}

async fn app_for(db: &TestDb, issuer: &str) -> Router {
    ensure_loopback_no_proxy();
    let mut state = AppState::with_memory_limiter(
        db.pool.clone(),
        Relay::new(RelayOptions::default()),
        test_config(),
    );
    state.oidc = Some(OidcRuntime::new(OidcConfig {
        issuer: issuer.to_string(),
        client_id: CLIENT_ID.to_string(),
        auth_material: ClientCred::new("test-cred".to_string()),
        redirect_url: format!("{}/api/auth/oidc/callback", issuer),
    }));
    router(state)
}

fn query_param(url: &str, name: &str) -> String {
    let parsed = openidconnect::url::Url::parse(url).expect("auth url");
    parsed
        .query_pairs()
        .find(|(key, _)| key.as_ref() == name)
        .map(|(_, value)| value.to_string())
        .expect("query param")
}

async fn start_flow(client: &mut Client) -> (String, String, String) {
    let (status, body, _cookie) = client.call("/api/auth/oidc/start").await;
    assert_eq!(status, StatusCode::OK);
    let auth_url = body["authorization_url"]
        .as_str()
        .expect("auth url")
        .to_string();
    (
        query_param(&auth_url, "state"),
        query_param(&auth_url, "nonce"),
        query_param(&auth_url, "code_challenge"),
    )
}

#[tokio::test]
async fn valid_login_creates_verified_user_and_session() {
    let db = TestDb::new().await;
    let (issuer, spec) = mock_provider();
    let app = app_for(&db, &issuer).await;
    let mut client = Client::new(app);

    let (status, body, binding) = client
        .call("/api/auth/oidc/start?return_to=/dashboard")
        .await;
    assert_eq!(status, StatusCode::OK);
    let auth_url = body["authorization_url"]
        .as_str()
        .expect("auth url")
        .to_string();
    assert!(binding
        .expect("binding cookie")
        .contains("businex_oidc_bind"));
    assert!(auth_url.contains("code_challenge"));
    assert!(auth_url.contains("scope=openid"));
    let state = query_param(&auth_url, "state");
    let nonce = query_param(&auth_url, "nonce");
    let challenge = query_param(&auth_url, "code_challenge");
    {
        let mut s = spec.lock().unwrap();
        s.nonce = nonce;
        s.expected_challenge = Some(challenge);
    }

    let (status, body, _sc) = client
        .call(&format!("/api/auth/oidc/callback?code=test-auth-code&state={}", state))
        .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(body["user"]["email"], json!("user42@example.test"));
    assert_eq!(body["return_to"], json!("/dashboard"));
    assert!(
        spec.lock().unwrap().seen_verifier.is_some(),
        "pkce verifier never reached provider"
    );

    let (status, me, _) = client.call("/api/auth/me").await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(me["user"]["email"], json!("user42@example.test"));

    let row: Option<(String,)> = sqlx::query_as(
        "SELECT user_id::text FROM oidc_identities WHERE issuer = $1 AND subject = $2",
    )
    .bind(issuer.trim_end_matches('/'))
    .bind("user-42")
    .fetch_optional(&db.pool)
    .await
    .expect("identity row");
    assert!(row.is_some(), "composite identity row missing");
}

#[tokio::test]
async fn stolen_flow_is_rejected_in_another_browser_and_not_consumed() {
    let db = TestDb::new().await;
    let (issuer, spec) = mock_provider();
    let app = app_for(&db, &issuer).await;

    let mut attacker = Client::new(app.clone());
    let (state, nonce, _challenge) = start_flow(&mut attacker).await;
    spec.lock().unwrap().nonce = nonce;
    let uri = format!("/api/auth/oidc/callback?code=test-auth-code&state={}", state);

    // Victim with no binding cookie at all.
    let mut victim = Client::new(app.clone());
    let (status, _body, _sc) = victim.call(&uri).await;
    assert_eq!(status, StatusCode::FORBIDDEN);

    // Victim presenting a different binding cookie.
    let mut victim2 = Client::new(app.clone());
    victim2.set_cookie("businex_oidc_bind=deadbeef".to_string());
    let (status, _body, _sc) = victim2.call(&uri).await;
    assert_eq!(status, StatusCode::FORBIDDEN);

    // Failed attempts must not consume the flow.
    let count: (i64,) = sqlx::query_as("SELECT count(*) FROM oidc_flows")
        .fetch_one(&db.pool)
        .await
        .expect("flow count");
    assert_eq!(count.0, 1, "flow consumed by a foreign browser");

    // The originating browser still completes.
    let (status, _body, _sc) = attacker.call(&uri).await;
    assert_eq!(status, StatusCode::OK);
}

#[tokio::test]
async fn replayed_state_is_refused() {
    let db = TestDb::new().await;
    let (issuer, spec) = mock_provider();
    let app = app_for(&db, &issuer).await;
    let mut client = Client::new(app);
    let (state, nonce, _challenge) = start_flow(&mut client).await;
    spec.lock().unwrap().nonce = nonce;
    let uri = format!("/api/auth/oidc/callback?code=test-auth-code&state={}", state);
    let (status, _body, _sc) = client.call(&uri).await;
    assert_eq!(status, StatusCode::OK);
    let (status, _body, _sc) = client.call(&uri).await;
    assert_eq!(status, StatusCode::FORBIDDEN, "replayed state was accepted");
}

async fn callback_status_with_spec<F>(edit: F) -> StatusCode
where
    F: FnOnce(&mut MockSpec),
{
    let db = TestDb::new().await;
    let (issuer, spec) = mock_provider();
    let app = app_for(&db, &issuer).await;
    let mut client = Client::new(app);
    let (state, nonce, _challenge) = start_flow(&mut client).await;
    {
        let mut s = spec.lock().unwrap();
        s.nonce = nonce;
        edit(&mut s);
    }
    let (status, _body, _sc) = client
        .call(&format!("/api/auth/oidc/callback?code=test-auth-code&state={}", state))
        .await;
    status
}

#[tokio::test]
async fn wrong_signature_is_rejected() {
    let status = callback_status_with_spec(|s| s.wrong_key = true).await;
    assert_eq!(status, StatusCode::FORBIDDEN);
}

#[tokio::test]
async fn wrong_issuer_is_rejected() {
    let status =
        callback_status_with_spec(|s| s.issuer_override = Some("http://evil.example".to_string()))
            .await;
    assert_eq!(status, StatusCode::FORBIDDEN);
}

#[tokio::test]
async fn wrong_audience_is_rejected() {
    let status =
        callback_status_with_spec(|s| s.aud_override = Some("other-client".to_string())).await;
    assert_eq!(status, StatusCode::FORBIDDEN);
}

#[tokio::test]
async fn wrong_nonce_is_rejected() {
    let status = callback_status_with_spec(|s| s.nonce = "tampered".to_string()).await;
    assert_eq!(status, StatusCode::FORBIDDEN);
}

#[tokio::test]
async fn expired_token_is_rejected() {
    let status = callback_status_with_spec(|s| s.exp_offset_secs = -3600).await;
    assert_eq!(status, StatusCode::FORBIDDEN);
}

#[tokio::test]
async fn unverified_email_is_rejected() {
    let status = callback_status_with_spec(|s| s.email_verified = false).await;
    assert_eq!(status, StatusCode::FORBIDDEN);
}

#[tokio::test]
async fn expired_flow_state_is_rejected() {
    let db = TestDb::new().await;
    let (issuer, spec) = mock_provider();
    let app = app_for(&db, &issuer).await;
    let mut client = Client::new(app);
    let (state, nonce, _challenge) = start_flow(&mut client).await;
    spec.lock().unwrap().nonce = nonce;
    let state_hash = hex::encode(Sha256::digest(state.as_bytes()));
    sqlx::query(
        "UPDATE oidc_flows SET expires_at = now() - interval '1 minute' WHERE state_hash = $1",
    )
    .bind(&state_hash)
    .execute(&db.pool)
    .await
    .expect("expire flow");
    let (status, _body, _sc) = client
        .call(&format!("/api/auth/oidc/callback?code=test-auth-code&state={}", state))
        .await;
    assert_eq!(status, StatusCode::FORBIDDEN);
}

#[tokio::test]
async fn hostile_return_to_falls_back_to_root() {
    let db = TestDb::new().await;
    let (issuer, spec) = mock_provider();
    let app = app_for(&db, &issuer).await;
    let mut client = Client::new(app);
    let (status, body, _binding) = client
        .call("/api/auth/oidc/start?return_to=//evil.example")
        .await;
    assert_eq!(status, StatusCode::OK);
    let auth_url = body["authorization_url"]
        .as_str()
        .expect("auth url")
        .to_string();
    let state = query_param(&auth_url, "state");
    spec.lock().unwrap().nonce = query_param(&auth_url, "nonce");
    let (status, body, _sc) = client
        .call(&format!("/api/auth/oidc/callback?code=test-auth-code&state={}", state))
        .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(body["return_to"], json!("/"));
}

#[tokio::test]
async fn same_subject_under_two_issuers_never_shares_an_account() {
    let db = TestDb::new().await;
    let (issuer_a, spec_a) = mock_provider();
    let (issuer_b, spec_b) = mock_provider();
    let app_a = app_for(&db, &issuer_a).await;
    let app_b = app_for(&db, &issuer_b).await;
    let mut client_a = Client::new(app_a);
    let mut client_b = Client::new(app_b);

    // Distinct verified e-mails per provider: shared addresses would collide
    // on the users unique index and prove nothing about account separation.
    spec_a.lock().unwrap().email = "user-a@example.test".to_string();
    spec_b.lock().unwrap().email = "user-b@example.test".to_string();
    let mut users = Vec::new();
    for (client, spec) in [(&mut client_a, &spec_a), (&mut client_b, &spec_b)] {
        let (state, nonce, _challenge) = start_flow(client).await;
        {
            let mut s = spec.lock().unwrap();
            s.nonce = nonce;
            s.subject = "stable-subject".to_string();
        }
        let (status, body, _sc) = client
            .call(&format!("/api/auth/oidc/callback?code=test-auth-code&state={}", state))
            .await;
        assert_eq!(status, StatusCode::OK);
        users.push(body["user"]["id"].as_str().expect("user id").to_string());
    }
    assert_ne!(users[0], users[1], "two issuers shared one account");

    let count: (i64,) = sqlx::query_as("SELECT count(*) FROM oidc_identities")
        .fetch_one(&db.pool)
        .await
        .expect("identity count");
    assert_eq!(count.0, 2, "composite mapping rows missing");
}

#[tokio::test]
async fn unconfigured_oidc_routes_fail_safely() {
    let db = TestDb::new().await;
    let state = AppState::with_memory_limiter(
        db.pool.clone(),
        Relay::new(RelayOptions::default()),
        test_config(),
    );
    let mut client = Client::new(router(state));
    let (status, body, _sc) = client.call("/api/auth/oidc/start").await;
    assert_eq!(status, StatusCode::NOT_FOUND);
    assert!(body["error"].as_str().is_some(), "error body missing");
}
