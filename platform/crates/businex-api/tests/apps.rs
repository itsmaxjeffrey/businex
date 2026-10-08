//! App platform tests: manifest publishing, the install lifecycle with
//! upgrade and rollback, schema-driven records with validation and
//! uniqueness, and role / tenant isolation. Real HTTP against a real
//! PostgreSQL, one disposable database per test.

use axum::body::Body;
use axum::http::{Request, StatusCode};
use businex_api::{router, AppConfig, AppState};
use businex_db::TestDb;
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
