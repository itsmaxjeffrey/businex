//! App platform tests: manifest publishing, the install lifecycle with
//! upgrade and rollback, schema-driven records with validation and
//! uniqueness, and role / tenant isolation. Real HTTP against a real
//! PostgreSQL, one disposable database per test.

use axum::body::Body;
use axum::http::{Request, StatusCode};
use base64::Engine as _;
use businex_api::builder::{GenerateOutcome, GenerateRequest, ManifestGenerator};
use businex_api::{router, AppConfig, AppState};
use businex_db::TestDb;
use businex_models::{Cost, MasterKey, ModelError, Usage};
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};
use businex_events::{Relay, RelayOptions};
use serde_json::{json, Value};
use tower::ServiceExt;
use uuid::Uuid;

async fn test_state() -> (TestDb, AppState) {
    let _ = tracing_subscriber::fmt().with_test_writer().try_init();
    let db = TestDb::new().await;
    let state = AppState::with_memory_limiter(
        db.pool.clone(),
        Relay::new(RelayOptions::default()),
        AppConfig {
            session_ttl_secs: 3600,
            cookie_secure: false,
            dev_login_enabled: true,
        },
    );
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

    async fn call(&mut self, method: &str, uri: &str, body: Option<Value>) -> (StatusCode, Value) {
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
        if let Some(sc) = resp
            .headers()
            .get("set-cookie")
            .and_then(|v| v.to_str().ok())
        {
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
        (status, value)
    }

    async fn get(&mut self, uri: &str) -> (StatusCode, Value) {
        self.call("GET", uri, None).await
    }

    async fn post(&mut self, uri: &str, body: Value) -> (StatusCode, Value) {
        self.call("POST", uri, Some(body)).await
    }

    async fn patch(&mut self, uri: &str, body: Value) -> (StatusCode, Value) {
        self.call("PATCH", uri, Some(body)).await
    }

    async fn put(&mut self, uri: &str, body: Value) -> (StatusCode, Value) {
        self.call("PUT", uri, Some(body)).await
    }

    async fn delete(&mut self, uri: &str) -> (StatusCode, Value) {
        self.call("DELETE", uri, None).await
    }
}

async fn register(app: axum::Router, tag: &str) -> (Client, String) {
    let mut client = Client::new(app);
    let addr = format!("{}-{}@example.test", tag, Uuid::new_v4());
    let (status, body) = client
        .post(
            "/api/auth/register",
            json!({"email": addr, "password": "correct horse battery", "name": tag}),
        )
        .await;
    assert_eq!(status, StatusCode::OK, "register: {}", body);
    (client, addr)
}

async fn create_company(client: &mut Client, name: &str) -> String {
    let (status, body) = client.post("/api/companies", json!({"name": name})).await;
    assert_eq!(status, StatusCode::OK, "create company: {}", body);
    body["id"].as_str().expect("company id").to_string()
}

fn manifest_v1() -> Value {
    json!({
        "schema_version": 1,
        "name": "Inventory",
        "slug": "inventory",
        "kind": "schema",
        "entities": [{
            "name": "item",
            "fields": [
                {"name": "sku", "type": "text", "required": true, "unique": true},
                {"name": "qty", "type": "number", "required": true}
            ]
        }],
        "permissions": ["records.read", "records.write"],
        "routes": [{"path": "/items", "method": "get", "permission": "records.read"}],
        "schedules": [],
        "dependencies": []
    })
}

fn manifest_v2() -> Value {
    let mut m = manifest_v1();
    m["entities"][0]["fields"]
        .as_array_mut()
        .expect("fields")
        .push(json!({"name": "note", "type": "text"}));
    m
}

#[tokio::test]
async fn app_lifecycle_upgrade_rollback_and_uninstall_preserve_records() {
    let (db, state) = test_state().await;
    let (mut client, _addr) = register(router(state), "owner").await;
    let company = create_company(&mut client, "Acme").await;
    let apps = format!("/api/companies/{}/apps", company);

    // Publish v1 and install it.
    let (status, body) = client.post(&apps, json!({"manifest": manifest_v1()})).await;
    assert_eq!(status, StatusCode::OK, "publish v1: {}", body);
    assert_eq!(body["version"], 1);
    let app_id = body["id"].as_str().expect("app id").to_string();

    let (status, body) = client
        .post(
            &format!("{}/{}/install", apps, app_id),
            json!({"version": 1}),
        )
        .await;
    assert_eq!(status, StatusCode::OK, "install v1: {}", body);
    assert_eq!(body["action"], "install");

    // A record under the v1 schema.
    let (status, body) = client
        .post(
            &format!("{}/{}/records/item", apps, app_id),
            json!({"data": {"sku": "A-1", "qty": 5}}),
        )
        .await;
    assert_eq!(status, StatusCode::OK, "create record: {}", body);

    // Publish v2 and upgrade the install to it.
    let (status, body) = client
        .post(
            &format!("{}/{}/versions", apps, app_id),
            json!({"manifest": manifest_v2()}),
        )
        .await;
    assert_eq!(status, StatusCode::OK, "publish v2: {}", body);
    assert_eq!(body["version"], 2);

    let (status, body) = client
        .post(
            &format!("{}/{}/install", apps, app_id),
            json!({"version": 2}),
        )
        .await;
    assert_eq!(status, StatusCode::OK, "upgrade: {}", body);
    assert_eq!(body["action"], "upgrade");

    // The record survives the upgrade.
    let (status, body) = client
        .get(&format!("{}/{}/records/item", apps, app_id))
        .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(body["records"].as_array().expect("records").len(), 1);
    assert_eq!(body["records"][0]["data"]["sku"], "A-1");

    // Rollback to v1.
    let (status, body) = client
        .post(
            &format!("{}/{}/install", apps, app_id),
            json!({"version": 1}),
        )
        .await;
    assert_eq!(status, StatusCode::OK, "rollback: {}", body);
    assert_eq!(body["action"], "rollback");

    // Uninstall, then reinstall: the record must still be there.
    let (status, body) = client.delete(&format!("{}/{}/install", apps, app_id)).await;
    assert_eq!(status, StatusCode::OK, "uninstall: {}", body);
    assert_eq!(body["uninstalled"], true);

    let (status, _body) = client
        .get(&format!("{}/{}/records/item", apps, app_id))
        .await;
    assert_eq!(
        status,
        StatusCode::CONFLICT,
        "records are not served while uninstalled"
    );

    let (status, body) = client
        .post(
            &format!("{}/{}/install", apps, app_id),
            json!({"version": 1}),
        )
        .await;
    assert_eq!(status, StatusCode::OK, "reinstall: {}", body);
    assert_eq!(body["action"], "install");

    let (status, body) = client
        .get(&format!("{}/{}/records/item", apps, app_id))
        .await;
    assert_eq!(status, StatusCode::OK, "records after reinstall: {}", body);
    let records = body["records"].as_array().expect("records");
    assert_eq!(records.len(), 1);
    assert_eq!(records[0]["data"]["sku"], "A-1");
    assert_eq!(records[0]["data"]["qty"], 5);

    // History records every transition in order.
    let (status, body) = client.get(&format!("{}/{}/history", apps, app_id)).await;
    assert_eq!(status, StatusCode::OK);
    let actions: Vec<String> = body["history"]
        .as_array()
        .expect("history")
        .iter()
        .map(|e| e["action"].as_str().expect("action").to_string())
        .collect();
    assert_eq!(
        actions,
        vec!["install", "upgrade", "rollback", "uninstall", "install"]
    );

    // Published versions are immutable at the storage layer too.
    let update = sqlx::query("UPDATE app_versions SET bundle_hash = bundle_hash WHERE app_id = $1")
        .bind(Uuid::parse_str(&app_id).expect("app uuid"))
        .execute(&db.pool)
        .await;
    assert!(update.is_err(), "app_versions rows must be immutable");
}

#[tokio::test]
async fn records_enforce_schema_and_uniqueness() {
    let (_db, state) = test_state().await;
    let (mut client, _addr) = register(router(state), "owner").await;
    let company = create_company(&mut client, "Acme").await;
    let apps = format!("/api/companies/{}/apps", company);

    let (status, body) = client.post(&apps, json!({"manifest": manifest_v1()})).await;
    assert_eq!(status, StatusCode::OK, "{}", body);
    let app_id = body["id"].as_str().expect("app id").to_string();
    let (status, _) = client
        .post(
            &format!("{}/{}/install", apps, app_id),
            json!({"version": 1}),
        )
        .await;
    assert_eq!(status, StatusCode::OK);
    let records = format!("{}/{}/records/item", apps, app_id);

    // Missing required field.
    let (status, body) = client.post(&records, json!({"data": {"sku": "A-1"}})).await;
    assert_eq!(status, StatusCode::BAD_REQUEST, "{}", body);

    // Unknown field.
    let (status, body) = client
        .post(
            &records,
            json!({"data": {"sku": "A-1", "qty": 1, "bogus": 2}}),
        )
        .await;
    assert_eq!(status, StatusCode::BAD_REQUEST, "{}", body);

    // Wrong type.
    let (status, body) = client
        .post(&records, json!({"data": {"sku": "A-1", "qty": "five"}}))
        .await;
    assert_eq!(status, StatusCode::BAD_REQUEST, "{}", body);

    // Valid create, then uniqueness violations on create and update.
    let (status, body) = client
        .post(&records, json!({"data": {"sku": "A-1", "qty": 1}}))
        .await;
    assert_eq!(status, StatusCode::OK, "{}", body);
    let first_id = body["id"].as_str().expect("record id").to_string();

    let (status, body) = client
        .post(&records, json!({"data": {"sku": "A-1", "qty": 2}}))
        .await;
    assert_eq!(status, StatusCode::CONFLICT, "{}", body);

    let (status, body) = client
        .post(&records, json!({"data": {"sku": "B-2", "qty": 3}}))
        .await;
    assert_eq!(status, StatusCode::OK, "{}", body);
    let second_id = body["id"].as_str().expect("record id").to_string();

    let (status, body) = client
        .patch(
            &format!("{}/{}", records, second_id),
            json!({"data": {"sku": "A-1", "qty": 3}}),
        )
        .await;
    assert_eq!(status, StatusCode::CONFLICT, "{}", body);

    // A record can update itself onto its own unique value.
    let (status, body) = client
        .patch(
            &format!("{}/{}", records, first_id),
            json!({"data": {"sku": "A-1", "qty": 9}}),
        )
        .await;
    assert_eq!(status, StatusCode::OK, "{}", body);
    assert_eq!(body["data"]["qty"], 9);
}

#[tokio::test]
async fn app_management_requires_admin_and_stays_inside_the_tenant() {
    let (_db, state) = test_state().await;
    let (mut owner, owner_addr) = register(router(state.clone()), "owner").await;
    let company_a = create_company(&mut owner, "Acme").await;
    let apps_a = format!("/api/companies/{}/apps", company_a);

    // A member of the company can see the library but cannot manage it.
    let (mut member, member_addr) = register(router(state.clone()), "member").await;
    let (status, body) = owner
        .post(
            &format!("/api/companies/{}/invitations", company_a),
            json!({"email": member_addr, "role": "member"}),
        )
        .await;
    assert_eq!(status, StatusCode::OK, "invite: {}", body);
    let token = body["token"].as_str().expect("token").to_string();
    let (status, body) = member
        .post("/api/invitations/accept", json!({"token": token}))
        .await;
    assert_eq!(status, StatusCode::OK, "accept: {}", body);

    let (status, _body) = member.get(&apps_a).await;
    assert_eq!(status, StatusCode::OK, "member can read the library");

    let (status, body) = member
        .post(&apps_a, json!({"manifest": manifest_v1()}))
        .await;
    assert_eq!(status, StatusCode::FORBIDDEN, "member publish: {}", body);

    // A signed-in user with no membership gets nothing at all.
    let (mut outsider, _addr) = register(router(state.clone()), "outsider").await;
    let (status, body) = outsider.get(&apps_a).await;
    assert_eq!(status, StatusCode::FORBIDDEN, "outsider: {}", body);
    let (status, body) = outsider
        .post(&apps_a, json!({"manifest": manifest_v1()}))
        .await;
    assert_eq!(status, StatusCode::FORBIDDEN, "outsider publish: {}", body);

    // And the owner's own record writes stay inside the tenant boundary.
    let (status, body) = owner
        .post(&apps_a, json!({"manifest": manifest_v1()}))
        .await;
    assert_eq!(status, StatusCode::OK, "owner publish: {}", body);
    let app_id = body["id"].as_str().expect("app id").to_string();
    let (status, _) = owner
        .post(
            &format!("{}/{}/install", apps_a, app_id),
            json!({"version": 1}),
        )
        .await;
    assert_eq!(status, StatusCode::OK);

    // The member can write records (records.write is a member permission).
    let (status, body) = member
        .post(
            &format!("{}/{}/records/item", apps_a, app_id),
            json!({"data": {"sku": "M-1", "qty": 1}}),
        )
        .await;
    assert_eq!(status, StatusCode::OK, "member record write: {}", body);
    let _ = owner_addr;
}

// Version publishing is serialized per app: two concurrent publishes both
// succeed with distinct consecutive version numbers instead of racing on
// max(version) + 1. All writes run through begin_company_tx, which pins
// SET LOCAL ROLE businex_app - the restricted application role.
#[tokio::test]
async fn concurrent_publishes_serialize_version_numbers() {
    let (_db, state) = test_state().await;
    let (mut owner, _addr) = register(router(state.clone()), "owner").await;
    let company = create_company(&mut owner, "Acme").await;
    let apps = format!("/api/companies/{}/apps", company);
    let (status, body) = owner.post(&apps, json!({"manifest": manifest_v1()})).await;
    assert_eq!(status, StatusCode::OK, "{}", body);
    let app_id = body["id"].as_str().expect("app id").to_string();
    let versions = format!("{}/{}/versions", apps, app_id);

    let mut first = Client::new(owner.app.clone());
    first.cookie = owner.cookie.clone();
    let mut second = Client::new(owner.app.clone());
    second.cookie = owner.cookie.clone();
    let (a, b) = tokio::join!(
        first.post(&versions, json!({"manifest": manifest_v2()})),
        second.post(&versions, json!({"manifest": manifest_v2()}))
    );
    assert_eq!(a.0, StatusCode::OK, "{:?}", a.1);
    assert_eq!(b.0, StatusCode::OK, "{:?}", b.1);
    let mut numbers = [
        a.1["version"].as_i64().expect("version a"),
        b.1["version"].as_i64().expect("version b"),
    ];
    numbers.sort_unstable();
    assert_eq!(numbers, [2, 3], "serialized consecutive versions: {:?}", numbers);
}

// Generation is an admin action like key management: members and outsiders
// are refused before anything dispatches.
#[tokio::test]
async fn generation_requires_admin_rights() {
    let fake = Arc::new(FakeGenerator::ok());
    let (_db, state) = model_test_state(fake.clone()).await;
    let (mut owner, _addr) = register(router(state.clone()), "owner").await;
    let company = create_company(&mut owner, "Acme").await;
    let generate_uri = format!("/api/companies/{}/apps/generate", company);
    let request_body = json!({"description": "An inventory tracker", "provider": "openai", "model": "test-model"});

    let (mut member, member_addr) = register(router(state.clone()), "member").await;
    let (status, invite) = owner
        .post(
            &format!("/api/companies/{}/invitations", company),
            json!({"email": member_addr, "role": "member"}),
        )
        .await;
    assert_eq!(status, StatusCode::OK, "{}", invite);
    let token = invite
        .get("token")
        .and_then(|v| v.as_str())
        .expect("token")
        .to_string();
    let (status, _) = member
        .post("/api/invitations/accept", json!({"token": token}))
        .await;
    assert_eq!(status, StatusCode::OK);

    let (status, body) = member.post(&generate_uri, request_body.clone()).await;
    assert_eq!(status, StatusCode::FORBIDDEN, "{}", body);

    let (mut outsider, _addr) = register(router(state.clone()), "outsider").await;
    let (status, body) = outsider.post(&generate_uri, request_body.clone()).await;
    assert_eq!(status, StatusCode::FORBIDDEN, "{}", body);

    assert_eq!(
        fake.calls.load(Ordering::SeqCst),
        0,
        "no dispatch for denied callers"
    );
}

// Two concurrent updates racing onto the same unique value admit exactly
// one: the per-value lock in ensure_unique serializes the update path's
// check-and-write just like the create path.
#[tokio::test]
async fn simultaneous_updates_cannot_duplicate_a_unique_value() {
    let (_db, state) = test_state().await;
    let (mut owner, _addr) = register(router(state.clone()), "owner").await;
    let company = create_company(&mut owner, "Acme").await;
    let apps = format!("/api/companies/{}/apps", company);
    let (status, body) = owner.post(&apps, json!({"manifest": manifest_v1()})).await;
    assert_eq!(status, StatusCode::OK, "{}", body);
    let app_id = body["id"].as_str().expect("app id").to_string();
    let (status, _) = owner
        .post(
            &format!("{}/{}/install", apps, app_id),
            json!({"version": 1}),
        )
        .await;
    assert_eq!(status, StatusCode::OK);
    let records = format!("{}/{}/records/item", apps, app_id);

    let mut ids = Vec::new();
    for sku in ["A-1", "B-2"] {
        let (status, body) = owner
            .post(&records, json!({"data": {"sku": sku, "qty": 1}}))
            .await;
        assert_eq!(status, StatusCode::OK, "{}", body);
        ids.push(body["id"].as_str().expect("record id").to_string());
    }

    let mut first = Client::new(owner.app.clone());
    first.cookie = owner.cookie.clone();
    let mut second = Client::new(owner.app.clone());
    second.cookie = owner.cookie.clone();
    let uri_a = format!("{}/{}", records, ids[0]);
    let uri_b = format!("{}/{}", records, ids[1]);
    let (a, b) = tokio::join!(
        first.patch(&uri_a, json!({"data": {"sku": "RACE-9", "qty": 1}})),
        second.patch(&uri_b, json!({"data": {"sku": "RACE-9", "qty": 2}}))
    );
    let statuses = [a.0, b.0];
    assert_eq!(
        statuses.iter().filter(|s| **s == StatusCode::OK).count(),
        1,
        "exactly one update wins: {:?}",
        statuses
    );
    assert!(
        statuses.contains(&StatusCode::CONFLICT),
        "the loser gets a conflict: {:?}",
        statuses
    );

    let (status, body) = owner.get(&records).await;
    assert_eq!(status, StatusCode::OK);
    let carriers = body["records"]
        .as_array()
        .expect("records")
        .iter()
        .filter(|r| r["data"]["sku"] == "RACE-9")
        .count();
    assert_eq!(carriers, 1, "exactly one record carries the barcode");
}

// The concurrency proofs drive the real API handlers, which write inside
// begin_company_tx. That helper pins the restricted application role; prove
// the pin is what those transactions actually run under.
#[tokio::test]
async fn app_transactions_pin_the_restricted_application_role() {
    let (db, state) = test_state().await;
    let (mut owner, _addr) = register(router(state.clone()), "owner").await;
    let company = create_company(&mut owner, "Acme").await;
    let company_id = Uuid::parse_str(&company).expect("company uuid");
    let ctx = businex_db::CompanyContext::with_actor(company_id, Uuid::nil());
    let mut tx = businex_db::begin_company_tx(&db.pool, ctx)
        .await
        .expect("company tx");
    let role: (String,) = sqlx::query_as("SELECT current_role")
        .fetch_one(&mut *tx)
        .await
        .expect("role");
    assert_eq!(
        role.0, "businex_app",
        "record writes land under the restricted role"
    );
}

// Two writers racing on the same unique value must produce exactly one
// record: the transaction-scoped lock in ensure_unique serializes the
// check-and-write so the loser sees the winner's row.
#[tokio::test]
async fn simultaneous_unique_writers_admit_exactly_one_record() {
    let (_db, state) = test_state().await;
    let (mut owner, _addr) = register(router(state.clone()), "owner").await;
    let company = create_company(&mut owner, "Acme").await;
    let apps = format!("/api/companies/{}/apps", company);

    let (status, body) = owner.post(&apps, json!({"manifest": manifest_v1()})).await;
    assert_eq!(status, StatusCode::OK, "{}", body);
    let app_id = body["id"].as_str().expect("app id").to_string();
    let (status, _) = owner
        .post(
            &format!("{}/{}/install", apps, app_id),
            json!({"version": 1}),
        )
        .await;
    assert_eq!(status, StatusCode::OK);
    let records = format!("{}/{}/records/item", apps, app_id);

    let mut first = Client::new(owner.app.clone());
    first.cookie = owner.cookie.clone();
    let mut second = Client::new(owner.app.clone());
    second.cookie = owner.cookie.clone();
    let (a, b) = tokio::join!(
        first.post(&records, json!({"data": {"sku": "RACE-1", "qty": 1}})),
        second.post(&records, json!({"data": {"sku": "RACE-1", "qty": 2}}))
    );
    let statuses = [a.0, b.0];
    let winners = statuses.iter().filter(|s| **s == StatusCode::OK).count();
    assert_eq!(winners, 1, "exactly one writer wins: {:?}", statuses);
    assert!(
        statuses.contains(&StatusCode::CONFLICT),
        "the loser gets a conflict: {:?}",
        statuses
    );

    // The store holds exactly one record for that barcode.
    let (status, body) = owner.get(&records).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(body["records"].as_array().expect("records").len(), 1);
}

// A version switch may never strand retained data: incompatible targets are
// refused up front and the install stays exactly where it was.
#[tokio::test]
async fn version_switches_refuse_changes_that_invalidate_retained_records() {
    let (_db, state) = test_state().await;
    let (mut owner, _addr) = register(router(state.clone()), "owner").await;
    let company = create_company(&mut owner, "Acme").await;
    let apps = format!("/api/companies/{}/apps", company);

    let (status, body) = owner.post(&apps, json!({"manifest": manifest_v1()})).await;
    assert_eq!(status, StatusCode::OK, "{}", body);
    let app_id = body["id"].as_str().expect("app id").to_string();
    let (status, _) = owner
        .post(
            &format!("{}/{}/install", apps, app_id),
            json!({"version": 1}),
        )
        .await;
    assert_eq!(status, StatusCode::OK);
    let records = format!("{}/{}/records/item", apps, app_id);
    let (status, body) = owner
        .post(&records, json!({"data": {"sku": "A-1", "qty": 5}}))
        .await;
    assert_eq!(status, StatusCode::OK, "{}", body);

    // v2 retypes qty from number to text: the retained record no longer fits.
    let mut breaking = manifest_v1();
    breaking["entities"][0]["fields"][1]["type"] = json!("text");
    let (status, body) = owner
        .post(
            &format!("{}/{}/versions", apps, app_id),
            json!({"manifest": breaking}),
        )
        .await;
    assert_eq!(status, StatusCode::OK, "{}", body);
    let (status, body) = owner
        .post(
            &format!("{}/{}/install", apps, app_id),
            json!({"version": 2}),
        )
        .await;
    assert_eq!(status, StatusCode::CONFLICT, "{}", body);
    assert!(
        body["error"].as_str().unwrap_or("").contains("do not fit"),
        "{}",
        body
    );

    // Data is untouched and no transition was recorded.
    let (status, body) = owner.get(&records).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(body["records"][0]["data"]["qty"], 5);
    let (status, body) = owner.get(&format!("{}/{}/history", apps, app_id)).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(
        body["history"].as_array().expect("history").len(),
        1,
        "only the original install: {}",
        body
    );

    // A compatible v3 (adds an optional field) upgrades normally.
    let (status, _) = owner
        .post(
            &format!("{}/{}/versions", apps, app_id),
            json!({"manifest": manifest_v2()}),
        )
        .await;
    assert_eq!(status, StatusCode::OK);
    let (status, body) = owner
        .post(
            &format!("{}/{}/install", apps, app_id),
            json!({"version": 3}),
        )
        .await;
    assert_eq!(status, StatusCode::OK, "{}", body);

    // Rollback is refused the moment data carries fields v1 rejects.
    let (status, body) = owner
        .post(
            &records,
            json!({"data": {"sku": "B-2", "qty": 1, "note": "late"}}),
        )
        .await;
    assert_eq!(status, StatusCode::OK, "{}", body);
    let (status, body) = owner
        .post(
            &format!("{}/{}/install", apps, app_id),
            json!({"version": 1}),
        )
        .await;
    assert_eq!(status, StatusCode::CONFLICT, "{}", body);
}

// Deleting is gated exactly like the other record paths: no live install, no
// delete — and an unknown entity is a plain 404.
#[tokio::test]
async fn deleting_records_requires_a_live_install() {
    let (_db, state) = test_state().await;
    let (mut owner, _addr) = register(router(state.clone()), "owner").await;
    let company = create_company(&mut owner, "Acme").await;
    let apps = format!("/api/companies/{}/apps", company);

    let (status, body) = owner.post(&apps, json!({"manifest": manifest_v1()})).await;
    assert_eq!(status, StatusCode::OK, "{}", body);
    let app_id = body["id"].as_str().expect("app id").to_string();
    let (status, _) = owner
        .post(
            &format!("{}/{}/install", apps, app_id),
            json!({"version": 1}),
        )
        .await;
    assert_eq!(status, StatusCode::OK);
    let records = format!("{}/{}/records/item", apps, app_id);
    let (status, body) = owner
        .post(&records, json!({"data": {"sku": "A-1", "qty": 5}}))
        .await;
    assert_eq!(status, StatusCode::OK, "{}", body);
    let record_id = body["id"].as_str().expect("record id").to_string();

    let (status, body) = owner
        .delete(&format!("{}/{}/install", apps, app_id))
        .await;
    assert_eq!(status, StatusCode::OK, "{}", body);

    // Delete is refused while uninstalled, and every other verb fails the
    // same way: one consistent "app is not installed" refusal.
    let (status, body) = owner.delete(&format!("{}/{}", records, record_id)).await;
    assert_eq!(status, StatusCode::CONFLICT, "{}", body);
    let (status, body) = owner.get(&records).await;
    assert_eq!(status, StatusCode::CONFLICT, "{}", body);
    let (status, body) = owner
        .post(&records, json!({"data": {"sku": "B-2", "qty": 1}}))
        .await;
    assert_eq!(status, StatusCode::CONFLICT, "{}", body);
    let (status, body) = owner
        .patch(
            &format!("{}/{}", records, record_id),
            json!({"data": {"qty": 9}}),
        )
        .await;
    assert_eq!(status, StatusCode::CONFLICT, "{}", body);

    let (status, _) = owner
        .post(
            &format!("{}/{}/install", apps, app_id),
            json!({"version": 1}),
        )
        .await;
    assert_eq!(status, StatusCode::OK);
    let (status, body) = owner.get(&records).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(body["records"].as_array().expect("records").len(), 1);

    // An entity the manifest does not declare is a 404, never a silent pass.
    let (status, _) = owner
        .delete(&format!("{}/{}/records/nope/{}", apps, app_id, record_id))
        .await;
    assert_eq!(status, StatusCode::NOT_FOUND);
}

// Entity-action rules narrow the platform permissions per entity and action:
// warehouse members write items but only managers write stocktakes, and only
// admins delete them.
#[tokio::test]
async fn entity_action_rules_narrow_writes_by_role() {
    let (_db, state) = test_state().await;
    let (mut owner, _addr) = register(router(state.clone()), "owner").await;
    let company = create_company(&mut owner, "Warehouse").await;
    let apps = format!("/api/companies/{}/apps", company);

    let manifest = json!({
        "schema_version": 1,
        "name": "Warehouse",
        "slug": "warehouse",
        "kind": "schema",
        "entities": [
            {
                "name": "item",
                "fields": [{"name": "sku", "type": "text", "required": true}],
                "access": {"read": "viewer", "write": "member", "delete": "manager"}
            },
            {
                "name": "stocktake",
                "fields": [{"name": "note", "type": "text", "required": true}],
                "access": {"read": "manager", "write": "manager", "delete": "admin"}
            }
        ],
        "permissions": ["records.read", "records.write", "records.delete"],
        "routes": [],
        "schedules": [],
        "dependencies": []
    });
    let (status, body) = owner.post(&apps, json!({"manifest": manifest})).await;
    assert_eq!(status, StatusCode::OK, "{}", body);
    let app_id = body["id"].as_str().expect("app id").to_string();
    let (status, _) = owner
        .post(
            &format!("{}/{}/install", apps, app_id),
            json!({"version": 1}),
        )
        .await;
    assert_eq!(status, StatusCode::OK);

    let (mut member, member_addr) = register(router(state.clone()), "member").await;
    let (status, body) = owner
        .post(
            &format!("/api/companies/{}/invitations", company),
            json!({"email": member_addr, "role": "member"}),
        )
        .await;
    assert_eq!(status, StatusCode::OK, "{}", body);
    let token = body["token"].as_str().expect("token").to_string();
    let (status, body) = member
        .post("/api/invitations/accept", json!({"token": token}))
        .await;
    assert_eq!(status, StatusCode::OK, "{}", body);

    let (mut manager, manager_addr) = register(router(state.clone()), "manager").await;
    let (status, body) = owner
        .post(
            &format!("/api/companies/{}/invitations", company),
            json!({"email": manager_addr, "role": "manager"}),
        )
        .await;
    assert_eq!(status, StatusCode::OK, "{}", body);
    let token = body["token"].as_str().expect("token").to_string();
    let (status, body) = manager
        .post("/api/invitations/accept", json!({"token": token}))
        .await;
    assert_eq!(status, StatusCode::OK, "{}", body);

    let item = format!("{}/{}/records/item", apps, app_id);
    let stocktake = format!("{}/{}/records/stocktake", apps, app_id);

    // Members write items (their floor is the member role).
    let (status, body) = member
        .post(&item, json!({"data": {"sku": "A-1"}}))
        .await;
    assert_eq!(status, StatusCode::OK, "{}", body);

    // But stocktakes are manager-only even though members hold records.write.
    let (status, body) = member
        .post(&stocktake, json!({"data": {"note": "try"}}))
        .await;
    assert_eq!(status, StatusCode::FORBIDDEN, "{}", body);
    assert!(
        body["error"].as_str().unwrap_or("").contains("manager"),
        "{}",
        body
    );
    let (status, _) = manager
        .post(&stocktake, json!({"data": {"note": "counted"}}))
        .await;
    assert_eq!(status, StatusCode::OK);

    // The read floor narrows listing the same way.
    let (status, body) = member.get(&stocktake).await;
    assert_eq!(status, StatusCode::FORBIDDEN, "{}", body);
    let (status, _) = manager.get(&stocktake).await;
    assert_eq!(status, StatusCode::OK);

    // Deletes are admin-only here: the manager holds records.delete but the
    // entity floor still refuses.
    let (status, body) = manager.get(&stocktake).await;
    assert_eq!(status, StatusCode::OK, "{}", body);
    let stock_id = body["records"][0]["id"].as_str().expect("id").to_string();
    let (status, body) = manager
        .delete(&format!("{}/{}", stocktake, stock_id))
        .await;
    assert_eq!(status, StatusCode::FORBIDDEN, "{}", body);
    let (status, _) = owner
        .delete(&format!("{}/{}", stocktake, stock_id))
        .await;
    assert_eq!(status, StatusCode::OK);
}

// ---------------------------------------------------------------------------
// Model keys, budgets and the budget-aware generator.
// ---------------------------------------------------------------------------

/// Scripted generator standing in for the provider adapters. Records every
/// call and the unsealed key it was handed.
#[derive(Clone, Copy)]
enum Fail {
    /// Fails before any network I/O: nothing was ever dispatched.
    Config,
    /// Fails after dispatch: the call may have run at the provider.
    Provider,
}

struct FakeGenerator {
    text: String,
    fail: Option<Fail>,
    calls: AtomicUsize,
    seen_key: Mutex<Option<String>>,
    seen_endpoint: Mutex<Option<String>>,
}

impl FakeGenerator {
    fn ok() -> Self {
        FakeGenerator {
            text: manifest_v1().to_string(),
            fail: None,
            calls: AtomicUsize::new(0),
            seen_key: Mutex::new(None),
            seen_endpoint: Mutex::new(None),
        }
    }

    fn failing() -> Self {
        FakeGenerator {
            fail: Some(Fail::Provider),
            ..Self::ok()
        }
    }

    fn config_failure() -> Self {
        FakeGenerator {
            fail: Some(Fail::Config),
            ..Self::ok()
        }
    }

    fn garbage() -> Self {
        FakeGenerator {
            text: "not json at all".into(),
            ..Self::ok()
        }
    }
}

#[async_trait::async_trait]
impl ManifestGenerator for FakeGenerator {
    async fn generate(&self, request: GenerateRequest) -> Result<GenerateOutcome, ModelError> {
        self.calls.fetch_add(1, Ordering::SeqCst);
        *self.seen_key.lock().expect("lock") = Some(request.api_key);
        *self.seen_endpoint.lock().expect("lock") = request.endpoint.clone();
        match self.fail {
            Some(Fail::Config) => return Err(ModelError::Config("test config failure".into())),
            Some(Fail::Provider) => return Err(ModelError::Provider),
            None => {}
        }
        Ok(GenerateOutcome {
            text: self.text.clone(),
            usage: Some(Usage::of(100, 50)),
            cost: Cost::Unknown,
        })
    }
}

async fn model_test_state(fake: Arc<FakeGenerator>) -> (TestDb, AppState) {
    let (db, mut state) = test_state().await;
    let encoded = base64::engine::general_purpose::STANDARD.encode([7u8; 32]);
    state.master_key = Some(Arc::new(
        MasterKey::from_base64(encoded).expect("test master key"),
    ));
    state.generator = fake;
    (db, state)
}

// Key material is sealed before storage and never leaves the server: not in
// create responses, not in listings, not in the row itself.
#[tokio::test]
async fn model_keys_are_sealed_at_rest_and_never_returned() {
    let (db, state) = model_test_state(Arc::new(FakeGenerator::ok())).await;
    let (mut owner, _addr) = register(router(state.clone()), "owner").await;
    let company = create_company(&mut owner, "Acme").await;
    let keys = format!("/api/companies/{}/model-keys", company);

    let (status, body) = owner
        .post(
            &keys,
            json!({"provider": "openai", "label": "default", "key": "sk-super-secret"}),
        )
        .await;
    assert_eq!(status, StatusCode::OK, "{}", body);
    assert!(!body.to_string().contains("sk-super-secret"), "{}", body);
    let key_id = body["id"].as_str().expect("key id").to_string();

    // The row holds nonce and ciphertext under the tenant pin, never plaintext.
    let mut conn = db.pool.acquire().await.expect("connection");
    sqlx::query("SELECT set_config('businex.company_id', $1, false)")
        .bind(&company)
        .execute(&mut *conn)
        .await
        .expect("tenant pin");
    let row: (Vec<u8>, Vec<u8>) =
        sqlx::query_as("SELECT sealed_nonce, sealed_bytes FROM model_keys")
            .fetch_one(&mut *conn)
            .await
            .expect("sealed row");
    assert!(row.0.len() >= 12, "a nonce is stored");
    assert!(
        !String::from_utf8_lossy(&row.1).contains("sk-super-secret"),
        "ciphertext must not leak the key"
    );

    // Listings carry metadata only.
    let (status, body) = owner.get(&keys).await;
    assert_eq!(status, StatusCode::OK, "{}", body);
    assert!(!body.to_string().contains("sk-super-secret"), "{}", body);

    // Revocation is recorded and takes the key out of service.
    let (status, body) = owner.delete(&format!("{}/{}", keys, key_id)).await;
    assert_eq!(status, StatusCode::OK, "{}", body);
    let (status, body) = owner.get(&keys).await;
    assert_eq!(status, StatusCode::OK, "{}", body);
    assert!(
        body["keys"][0]["revoked_at"].is_string(),
        "revocation is visible: {}",
        body
    );
}

// Managing provider keys is an admin action, not a member action.
#[tokio::test]
async fn model_keys_require_admin_rights() {
    let (_db, state) = model_test_state(Arc::new(FakeGenerator::ok())).await;
    let (mut owner, _addr) = register(router(state.clone()), "owner").await;
    let company = create_company(&mut owner, "Acme").await;
    let keys = format!("/api/companies/{}/model-keys", company);

    let (mut member, member_addr) = register(router(state.clone()), "member").await;
    let (status, body) = owner
        .post(
            &format!("/api/companies/{}/invitations", company),
            json!({"email": member_addr, "role": "member"}),
        )
        .await;
    assert_eq!(status, StatusCode::OK, "{}", body);
    let token = body["token"].as_str().expect("token").to_string();
    let (status, _) = member
        .post("/api/invitations/accept", json!({"token": token}))
        .await;
    assert_eq!(status, StatusCode::OK);

    let (status, body) = member
        .post(&keys, json!({"provider": "openai", "label": "default", "key": "sk-1"}))
        .await;
    assert_eq!(status, StatusCode::FORBIDDEN, "{}", body);
    let (status, body) = member.get(&keys).await;
    assert_eq!(status, StatusCode::FORBIDDEN, "{}", body);
}

// The full generate path: unsealed key reaches the generator, the app exists
// afterwards, and the budget carries exactly the observed usage.
#[tokio::test]
async fn generate_settles_observed_usage_and_creates_the_app() {
    let fake = Arc::new(FakeGenerator::ok());
    let (_db, state) = model_test_state(fake.clone()).await;
    let (mut owner, _addr) = register(router(state.clone()), "owner").await;
    let company = create_company(&mut owner, "Acme").await;
    let keys = format!("/api/companies/{}/model-keys", company);
    let budget_uri = format!("/api/companies/{}/model-budget", company);

    let (status, body) = owner
        .post(
            &keys,
            json!({"provider": "openai", "label": "default", "key": "sk-secret-1"}),
        )
        .await;
    assert_eq!(status, StatusCode::OK, "{}", body);
    let (status, body) = owner
        .put(&budget_uri, json!({"max_tokens_per_period": 10000}))
        .await;
    assert_eq!(status, StatusCode::OK, "{}", body);

    let (status, body) = owner
        .post(
            &format!("/api/companies/{}/apps/generate", company),
            json!({
                "description": "An inventory tracker with stock items",
                "provider": "openai",
                "model": "test-model",
                "label": "default"
            }),
        )
        .await;
    assert_eq!(status, StatusCode::OK, "{}", body);
    assert_eq!(body["app"]["version"], 1);
    assert_eq!(body["usage"]["input_tokens"], 100);
    assert_eq!(body["usage"]["output_tokens"], 50);
    let app_id = body["app"]["id"].as_str().expect("app id").to_string();

    // The generator was handed the unsealed key, exactly once.
    assert_eq!(
        fake.seen_key.lock().expect("lock").as_deref(),
        Some("sk-secret-1")
    );
    assert_eq!(fake.calls.load(Ordering::SeqCst), 1);

    // The generated app really exists in the company library.
    let (status, body) = owner.get(&format!("/api/companies/{}/apps", company)).await;
    assert_eq!(status, StatusCode::OK, "{}", body);
    assert!(
        body.to_string().contains(&app_id),
        "generated app appears in the library: {}",
        body
    );

    // Budget settled with observed usage: 150 tokens, nothing left reserved,
    // and the unknown price counted as unknown rather than zero.
    let (status, body) = owner.get(&budget_uri).await;
    assert_eq!(status, StatusCode::OK, "{}", body);
    let budget = &body["budget"];
    assert_eq!(budget["settled_tokens"], 150);
    assert_eq!(budget["reserved_tokens"], 0);
    assert_eq!(budget["unknown_cost_calls"], 1);
}

// A denied budget means the model is never called at all.
#[tokio::test]
async fn generate_is_denied_before_dispatch_when_the_budget_is_spent() {
    let fake = Arc::new(FakeGenerator::ok());
    let (_db, state) = model_test_state(fake.clone()).await;
    let (mut owner, _addr) = register(router(state.clone()), "owner").await;
    let company = create_company(&mut owner, "Acme").await;
    let budget_uri = format!("/api/companies/{}/model-budget", company);

    let (status, body) = owner
        .post(
            &format!("/api/companies/{}/model-keys", company),
            json!({"provider": "openai", "label": "default", "key": "sk-1"}),
        )
        .await;
    assert_eq!(status, StatusCode::OK, "{}", body);
    // The pre-dispatch estimate for any description exceeds this cap.
    let (status, body) = owner
        .put(&budget_uri, json!({"max_tokens_per_period": 500}))
        .await;
    assert_eq!(status, StatusCode::OK, "{}", body);

    let (status, body) = owner
        .post(
            &format!("/api/companies/{}/apps/generate", company),
            json!({"description": "An inventory tracker", "provider": "openai", "model": "test-model"}),
        )
        .await;
    assert_eq!(status, StatusCode::FORBIDDEN, "{}", body);
    assert!(
        body["error"].as_str().unwrap_or("").contains("budget"),
        "{}",
        body
    );
    assert_eq!(
        fake.calls.load(Ordering::SeqCst),
        0,
        "the model was never called"
    );

    let (status, body) = owner.get(&format!("/api/companies/{}/apps", company)).await;
    assert_eq!(status, StatusCode::OK);
    assert!(
        !body.to_string().contains("inventory"),
        "no app was created: {}",
        body
    );
    let (status, body) = owner.get(&budget_uri).await;
    assert_eq!(status, StatusCode::OK, "{}", body);
    assert_eq!(body["budget"]["reserved_tokens"], 0, "{}", body);
    assert_eq!(body["budget"]["settled_tokens"], 0, "{}", body);
}

// A pre-dispatch configuration failure provably never reached the provider,
// so the reservation is released untouched.
#[tokio::test]
async fn a_pre_dispatch_failure_releases_its_reservation() {
    let fake = Arc::new(FakeGenerator::config_failure());
    let (_db, state) = model_test_state(fake.clone()).await;
    let (mut owner, _addr) = register(router(state.clone()), "owner").await;
    let company = create_company(&mut owner, "Acme").await;
    let budget_uri = format!("/api/companies/{}/model-budget", company);

    let (status, _) = owner
        .post(
            &format!("/api/companies/{}/model-keys", company),
            json!({"provider": "openai", "label": "default", "key": "sk-1"}),
        )
        .await;
    assert_eq!(status, StatusCode::OK);
    let (status, _) = owner
        .put(&budget_uri, json!({"max_tokens_per_period": 10000}))
        .await;
    assert_eq!(status, StatusCode::OK);

    let (status, body) = owner
        .post(
            &format!("/api/companies/{}/apps/generate", company),
            json!({"description": "An inventory tracker", "provider": "openai", "model": "test-model"}),
        )
        .await;
    assert_eq!(status, StatusCode::INTERNAL_SERVER_ERROR, "{}", body);
    assert_eq!(fake.calls.load(Ordering::SeqCst), 1);

    let (status, body) = owner.get(&budget_uri).await;
    assert_eq!(status, StatusCode::OK, "{}", body);
    assert_eq!(body["budget"]["reserved_tokens"], 0, "{}", body);
    assert_eq!(body["budget"]["settled_tokens"], 0, "{}", body);
}

// A failure after dispatch has an unknown outcome: the provider may have run
// the call. The estimate stays charged and the usage row records the unknown
// outcome for reconciliation instead of silently refunding the capacity.
#[tokio::test]
async fn an_ambiguous_failure_is_charged_and_recorded_for_reconciliation() {
    let fake = Arc::new(FakeGenerator::failing());
    let (db, state) = model_test_state(fake.clone()).await;
    let (mut owner, _addr) = register(router(state.clone()), "owner").await;
    let company = create_company(&mut owner, "Acme").await;
    let budget_uri = format!("/api/companies/{}/model-budget", company);

    let (status, _) = owner
        .post(
            &format!("/api/companies/{}/model-keys", company),
            json!({"provider": "openai", "label": "default", "key": "sk-1"}),
        )
        .await;
    assert_eq!(status, StatusCode::OK);
    let (status, _) = owner
        .put(&budget_uri, json!({"max_tokens_per_period": 10000}))
        .await;
    assert_eq!(status, StatusCode::OK);

    let (status, body) = owner
        .post(
            &format!("/api/companies/{}/apps/generate", company),
            json!({"description": "An inventory tracker", "provider": "openai", "model": "test-model"}),
        )
        .await;
    assert_eq!(status, StatusCode::BAD_GATEWAY, "{}", body);
    assert_eq!(fake.calls.load(Ordering::SeqCst), 1);

    // The conservative estimate stays charged; nothing was refunded.
    let expected = businex_api::builder::estimate_tokens("An inventory tracker");
    let (status, body) = owner.get(&budget_uri).await;
    assert_eq!(status, StatusCode::OK, "{}", body);
    assert_eq!(body["budget"]["reserved_tokens"], 0, "{}", body);
    assert_eq!(body["budget"]["settled_tokens"], expected, "{}", body);
    assert_eq!(body["budget"]["unknown_cost_calls"], 1, "{}", body);

    // And the row says the outcome is unknown, for reconciliation.
    let mut conn = db.pool.acquire().await.expect("connection");
    sqlx::query("SELECT set_config('businex.company_id', $1, false)")
        .bind(&company)
        .execute(&mut *conn)
        .await
        .expect("tenant pin");
    let outcome: (String,) =
        sqlx::query_as("SELECT outcome FROM model_usage WHERE company_id = $1::uuid")
            .bind(&company)
            .fetch_one(&mut *conn)
            .await
            .expect("usage row");
    assert_eq!(outcome.0, "ambiguous", "unknown outcome persisted");
}

// The endpoint is trusted configuration stored with the key: generation
// requests can never redirect a key anywhere else. An unauthorized endpoint
// receives nothing, not even a dispatch attempt.
#[tokio::test]
async fn generation_cannot_redirect_a_key_to_an_unauthorized_endpoint() {
    let fake = Arc::new(FakeGenerator::ok());
    let (_db, state) = model_test_state(fake.clone()).await;
    let (mut owner, _addr) = register(router(state.clone()), "owner").await;
    let company = create_company(&mut owner, "Acme").await;
    let keys = format!("/api/companies/{}/model-keys", company);
    let generate_uri = format!("/api/companies/{}/apps/generate", company);

    // Self-hosted keys must state their endpoint explicitly.
    let (status, body) = owner
        .post(&keys, json!({"provider": "xiaomi", "label": "local", "key": "sk-1"}))
        .await;
    assert_eq!(status, StatusCode::BAD_REQUEST, "{}", body);

    // And the endpoint must pass the egress policy: private targets die.
    let (status, body) = owner
        .post(
            &keys,
            json!({"provider": "xiaomi", "label": "local", "key": "sk-1",
                   "endpoint": "http://10.0.0.5:8080/v1"}),
        )
        .await;
    assert_eq!(status, StatusCode::BAD_REQUEST, "{}", body);

    // An explicit loopback endpoint is accepted for development self-hosting.
    let (status, body) = owner
        .post(
            &keys,
            json!({"provider": "xiaomi", "label": "local", "key": "sk-1",
                   "endpoint": "http://127.0.0.1:9999/v1"}),
        )
        .await;
    assert_eq!(status, StatusCode::OK, "{}", body);

    // Providers with fixed endpoints refuse overrides entirely.
    let (status, body) = owner
        .post(
            &keys,
            json!({"provider": "openai", "label": "cloud", "key": "sk-2",
                   "endpoint": "https://evil.example/v1"}),
        )
        .await;
    assert_eq!(status, StatusCode::BAD_REQUEST, "{}", body);

    // A generation request carrying its own endpoint is refused before any
    // dispatch: the unauthorized endpoint receives the key never.
    let (status, body) = owner
        .post(
            &generate_uri,
            json!({
                "description": "An inventory tracker",
                "provider": "xiaomi",
                "model": "test-model",
                "label": "local",
                "base_url": "https://evil.example/v1"
            }),
        )
        .await;
    assert!(status.is_client_error(), "{}", body);
    assert_eq!(
        fake.calls.load(Ordering::SeqCst),
        0,
        "nothing was dispatched"
    );
    assert!(
        fake.seen_key.lock().expect("lock").is_none(),
        "the key never left storage"
    );

    // The legitimate call carries exactly the stored endpoint and nothing
    // else; the key goes there and only there.
    let (status, body) = owner
        .post(
            &generate_uri,
            json!({
                "description": "An inventory tracker",
                "provider": "xiaomi",
                "model": "test-model",
                "label": "local"
            }),
        )
        .await;
    assert_eq!(status, StatusCode::OK, "{}", body);
    assert_eq!(
        fake.seen_endpoint.lock().expect("lock").as_deref(),
        Some("http://127.0.0.1:9999/v1")
    );
    assert_eq!(
        fake.seen_key.lock().expect("lock").as_deref(),
        Some("sk-1")
    );
}

// Unusable model output is still a real, charged call: the usage is settled
// and the response says exactly what went wrong.
#[tokio::test]
async fn unusable_model_output_is_charged_and_refused() {
    let fake = Arc::new(FakeGenerator::garbage());
    let (_db, state) = model_test_state(fake.clone()).await;
    let (mut owner, _addr) = register(router(state.clone()), "owner").await;
    let company = create_company(&mut owner, "Acme").await;
    let budget_uri = format!("/api/companies/{}/model-budget", company);

    let (status, _) = owner
        .post(
            &format!("/api/companies/{}/model-keys", company),
            json!({"provider": "openai", "label": "default", "key": "sk-1"}),
        )
        .await;
    assert_eq!(status, StatusCode::OK);
    let (status, _) = owner
        .put(&budget_uri, json!({"max_tokens_per_period": 10000}))
        .await;
    assert_eq!(status, StatusCode::OK);

    let (status, body) = owner
        .post(
            &format!("/api/companies/{}/apps/generate", company),
            json!({"description": "An inventory tracker", "provider": "openai", "model": "test-model"}),
        )
        .await;
    assert_eq!(status, StatusCode::BAD_GATEWAY, "{}", body);
    assert!(
        body["error"]
            .as_str()
            .unwrap_or("")
            .contains("valid app manifest"),
        "{}",
        body
    );
    assert_eq!(fake.calls.load(Ordering::SeqCst), 1);

    // The call happened, so its usage is settled exactly once.
    let (status, body) = owner.get(&budget_uri).await;
    assert_eq!(status, StatusCode::OK, "{}", body);
    assert_eq!(body["budget"]["settled_tokens"], 150, "{}", body);
    assert_eq!(body["budget"]["reserved_tokens"], 0, "{}", body);
    assert_eq!(body["budget"]["unknown_cost_calls"], 1, "{}", body);

    // But no half-built app was left behind.
    let (status, body) = owner.get(&format!("/api/companies/{}/apps", company)).await;
    assert_eq!(status, StatusCode::OK);
    assert!(
        !body.to_string().contains("inventory"),
        "no app was created: {}",
        body
    );
}
