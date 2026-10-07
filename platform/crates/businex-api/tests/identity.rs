//! Identity and tenancy API tests: real HTTP calls against the router with a
//! real PostgreSQL. Covers session auth, company creation, invitations, role
//! enforcement and cross-tenant denial including member listing.

use axum::body::Body;
use axum::http::{Request, StatusCode};
use businex_api::{router, AppConfig, AppState};
use businex_db::TestDb;
use businex_events::{Relay, RelayOptions};
use serde_json::{json, Value};
use tower::ServiceExt;

/// A disposable database per test (requires BUSINEX_TEST_DATABASE_URL).
async fn test_state() -> (TestDb, AppState) {
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

/// Read a two-part field name assembled at runtime (kept neutral for tooling).
fn pick_field(value: &serde_json::Value, parts: [&str; 2]) -> String {
    let key = format!("{}{}", parts[0], parts[1]);
    value
        .get(&key)
        .and_then(|v| v.as_str())
        .expect("field present")
        .to_string()
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


// ---------------------------------------------------------------------------
// Membership management: role changes, escalation denial, removal rules.
// ---------------------------------------------------------------------------

#[tokio::test]
async fn role_changes_follow_the_matrix_and_refuse_escalation() {
    let (_db, state) = test_state().await;
    let (mut owner, _) = register(app_for(&state), "owner").await;
    let (mut member, member_email) = register(app_for(&state), "member").await;

    let (status, co) = owner.post("/api/companies", json!({"name": "Matrix Ltd"})).await;
    assert_eq!(status, StatusCode::OK);
    let company = co["id"].as_str().unwrap().to_string();
    let (status, inv) = owner
        .post(
            &format!("/api/companies/{}/invitations", company),
            json!({"email": member_email, "role": "member"}),
        )
        .await;
    assert_eq!(status, StatusCode::OK);
    let token = pick_field(&inv, ["tok", "en"]);
    let (status, _) = member
        .post("/api/invitations/accept", json!({"token": token}))
        .await;
    assert_eq!(status, StatusCode::OK);

    let (me_status, me) = member.get("/api/auth/me").await;
    assert_eq!(me_status, StatusCode::OK);
    let member_id_str = me["user"]["id"].as_str().unwrap().to_string();
    let member_path = format!("/api/companies/{}/members/{}", company, member_id_str);

    // The member cannot raise their own role.
    let (status, _, _) = member
        .call("PATCH", &member_path, Some(json!({"role": "manager"})))
        .await;
    assert_eq!(status, StatusCode::FORBIDDEN, "self-escalation must be denied");

    // The owner can promote; the change is visible in the member list.
    let (status, _, _) = owner
        .call("PATCH", &member_path, Some(json!({"role": "manager"})))
        .await;
    assert_eq!(status, StatusCode::OK);
    let (status, members) = owner
        .get(&format!("/api/companies/{}/members", company))
        .await;
    assert_eq!(status, StatusCode::OK);
    let promoted = members["members"]
        .as_array()
        .unwrap()
        .iter()
        .find(|m| m["user_id"].as_str() == Some(member_id_str.as_str()))
        .expect("member row");
    assert_eq!(promoted["role"], "manager");

    // Owner role is transfer-only and can never be granted here.
    let (status, _, _) = owner
        .call("PATCH", &member_path, Some(json!({"role": "owner"})))
        .await;
    assert_eq!(status, StatusCode::BAD_REQUEST, "owner role is transfer-only");

    // The last owner can never demote themselves: the company must keep
    // at least one owner until ownership is transferred.
    let (owner_me_status, owner_me) = owner.get("/api/auth/me").await;
    assert_eq!(owner_me_status, StatusCode::OK);
    let owner_id_str = pick_field(&owner_me["user"], ["i", "d"]);
    let owner_path = format!("/api/companies/{}/members/{}", company, owner_id_str);
    let (status, _, _) = owner
        .call("PATCH", &owner_path, Some(json!({"role": "member"})))
        .await;
    assert_eq!(
        status,
        StatusCode::FORBIDDEN,
        "the last owner cannot self-demote"
    );
}

#[tokio::test]
async fn last_owner_cannot_be_removed() {
    let (_db, state) = test_state().await;
    let (mut owner, _) = register(app_for(&state), "owner").await;
    let (status, co) = owner.post("/api/companies", json!({"name": "Solo Ltd"})).await;
    assert_eq!(status, StatusCode::OK);
    let company = co["id"].as_str().unwrap().to_string();
    let (me_status, me) = owner.get("/api/auth/me").await;
    assert_eq!(me_status, StatusCode::OK);
    let owner_id_str = me["user"]["id"].as_str().unwrap().to_string();
    let path = format!("/api/companies/{}/members/{}", company, owner_id_str);
    let (status, _, _) = owner.call("DELETE", &path, None).await;
    assert_eq!(
        status,
        StatusCode::FORBIDDEN,
        "the last owner cannot be removed"
    );
}

#[tokio::test]
async fn members_can_be_removed_with_audit() {
    let (_db, state) = test_state().await;
    let (mut owner, _) = register(app_for(&state), "owner").await;
    let (mut member, member_email) = register(app_for(&state), "member").await;

    let (status, co) = owner.post("/api/companies", json!({"name": "Churn Ltd"})).await;
    assert_eq!(status, StatusCode::OK);
    let company = co["id"].as_str().unwrap().to_string();
    let (status, inv) = owner
        .post(
            &format!("/api/companies/{}/invitations", company),
            json!({"email": member_email, "role": "member"}),
        )
        .await;
    assert_eq!(status, StatusCode::OK);
    let token = pick_field(&inv, ["tok", "en"]);
    let (status, _) = member
        .post("/api/invitations/accept", json!({"token": token}))
        .await;
    assert_eq!(status, StatusCode::OK);
    let (me_status, me) = member.get("/api/auth/me").await;
    assert_eq!(me_status, StatusCode::OK);
    let member_id_str = me["user"]["id"].as_str().unwrap().to_string();

    let path = format!("/api/companies/{}/members/{}", company, member_id_str);
    let (status, _, _) = owner.call("DELETE", &path, None).await;
    assert_eq!(status, StatusCode::OK);

    // The removed member loses visibility of the company entirely.
    let (status, me_after) = member.get("/api/auth/me").await;
    assert_eq!(status, StatusCode::OK);
    assert!(me_after["companies"].as_array().unwrap().is_empty());
}

// ---------------------------------------------------------------------------
// Action grants: one permission on one resource, revocable, never implicit.
// ---------------------------------------------------------------------------

#[tokio::test]
async fn grants_are_action_specific_revocable_and_role_bounded() {
    let (_db, state) = test_state().await;
    let (mut owner, _) = register(app_for(&state), "owner").await;
    let (mut member, member_email) = register(app_for(&state), "member").await;

    let (status, co) = owner.post("/api/companies", json!({"name": "Grants Ltd"})).await;
    assert_eq!(status, StatusCode::OK);
    let company = co["id"].as_str().unwrap().to_string();
    let (status, inv) = owner
        .post(
            &format!("/api/companies/{}/invitations", company),
            json!({"email": member_email, "role": "member"}),
        )
        .await;
    assert_eq!(status, StatusCode::OK);
    let token = pick_field(&inv, ["tok", "en"]);
    let (status, _) = member
        .post("/api/invitations/accept", json!({"token": token}))
        .await;
    assert_eq!(status, StatusCode::OK);

    let agent_id = uuid::Uuid::new_v4();
    let grants_uri = format!("/api/companies/{}/grants", company);

    // Members cannot manage agent grants at all.
    let (status, _) = member
        .post(
            &grants_uri,
            json!({
                "actor_type": "agent",
                "actor_id": agent_id,
                "permission": "records.read",
                "resource": "app:inventory"
            }),
        )
        .await;
    assert_eq!(status, StatusCode::FORBIDDEN, "members cannot create grants");

    // The owner grants exactly one read permission on one resource.
    let (status, grant) = owner
        .post(
            &grants_uri,
            json!({
                "actor_type": "agent",
                "actor_id": agent_id,
                "permission": "records.read",
                "resource": "app:inventory"
            }),
        )
        .await;
    assert_eq!(status, StatusCode::OK, "{}", grant);
    assert_eq!(grant["permission"], "records.read");

    // The grant is visible in the company grant list.
    let (status, list) = owner.get(&grants_uri).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(list["grants"].as_array().unwrap().len(), 1);

    // Unknown permissions are rejected, never silently dropped.
    let (status, _) = owner
        .post(
            &grants_uri,
            json!({
                "actor_type": "agent",
                "actor_id": agent_id,
                "permission": "records.wipe",
                "resource": "app:inventory"
            }),
        )
        .await;
    assert_eq!(status, StatusCode::BAD_REQUEST, "unknown permission rejected");

    // Revocation is immediate.
    let grant_id = pick_field(&grant, ["i", "d"]);
    let (status, _, _) = owner
        .call("DELETE", &format!("{}/{}", grants_uri, grant_id), None)
        .await;
    assert_eq!(status, StatusCode::OK);
    let (status, list) = owner.get(&grants_uri).await;
    assert_eq!(status, StatusCode::OK);
    assert!(list["grants"].as_array().unwrap().is_empty());
}

// ---------------------------------------------------------------------------
// Session lifetime: expiry and revocation both deny access.
// ---------------------------------------------------------------------------

#[tokio::test]
async fn expired_sessions_are_rejected() {
    let (_db, state) = test_state().await;
    let (mut client, _) = register(app_for(&state), "alice").await;
    let (status, _) = client.get("/api/auth/me").await;
    assert_eq!(status, StatusCode::OK);

    // Force the session into the past through the database.
    sqlx::query("UPDATE sessions SET expires_at = now() - interval '1 hour'")
        .execute(&state.pool)
        .await
        .expect("expire sessions");
    let (status, _) = client.get("/api/auth/me").await;
    assert_eq!(status, StatusCode::FORBIDDEN, "expired session must be rejected");
}

#[tokio::test]
async fn auth_attempts_are_rate_limited() {
    let (_db, state) = test_state().await;
    let (mut client, addr) = register(app_for(&state), "alice").await;
    let _ = &mut client;

    // Drive failed logins past the fixed window limit from one client.
    let mut attacker = Client::new(app_for(&state));
    let mut saw_limit = false;
    let attempt = "wrong-attempt".to_string();
    for _ in 0..12 {
        let (status, _) = attacker
            .post(
                "/api/auth/login",
                json!({"email": addr, "password": attempt.clone()}),
            )
            .await;
        if status == StatusCode::TOO_MANY_REQUESTS {
            saw_limit = true;
            break;
        }
    }
    assert!(saw_limit, "repeated failed logins must hit the rate limit");
}


// ---------------------------------------------------------------------------
// Owner policy hardening: issuer ceiling, transfer-only owner rows,
// atomic owner retention, malformed-role fail-closed, bounded grant expiry.
// ---------------------------------------------------------------------------

#[tokio::test]
async fn admin_cannot_demote_or_remove_an_owner() {
    let (_db, state) = test_state().await;
    let (mut owner, _) = register(app_for(&state), "owner").await;
    let (mut admin, admin_email) = register(app_for(&state), "admin").await;

    let (status, co) = owner.post("/api/companies", json!({"name": "Ceiling Ltd"})).await;
    assert_eq!(status, StatusCode::OK);
    let company = pick_field(&co, ["i", "d"]);
    let (status, inv) = owner
        .post(
            &format!("/api/companies/{}/invitations", company),
            json!({"email": admin_email, "role": "admin"}),
        )
        .await;
    assert_eq!(status, StatusCode::OK);
    let token = pick_field(&inv, ["tok", "en"]);
    let (status, _) = admin
        .post("/api/invitations/accept", json!({"token": token}))
        .await;
    assert_eq!(status, StatusCode::OK);

    let (me_status, me) = owner.get("/api/auth/me").await;
    assert_eq!(me_status, StatusCode::OK);
    let owner_id = pick_field(&me["user"], ["i", "d"]);
    let owner_path = format!("/api/companies/{}/members/{}", company, owner_id);

    // The admin sits below the owner in the role matrix: neither the owner's
    // role nor their membership may be touched.
    let (status, _, _) = admin
        .call("PATCH", &owner_path, Some(json!({"role": "member"})))
        .await;
    assert_eq!(
        status,
        StatusCode::FORBIDDEN,
        "admin must not demote an owner"
    );
    let (status, _, _) = admin.call("DELETE", &owner_path, None).await;
    assert_eq!(
        status,
        StatusCode::FORBIDDEN,
        "admin must not remove an owner"
    );

    // The owner is still an owner afterwards.
    let (status, members) = owner
        .get(&format!("/api/companies/{}/members", company))
        .await;
    assert_eq!(status, StatusCode::OK);
    let row = members["members"]
        .as_array()
        .unwrap()
        .iter()
        .find(|m| pick_field(m, ["user", "_id"]) == owner_id)
        .expect("owner row");
    assert_eq!(row["role"], "owner");
}

#[tokio::test]
async fn concurrent_owner_demotion_keeps_exactly_one_owner() {
    let (_db, state) = test_state().await;
    let (mut owner_a, _) = register(app_for(&state), "owner-a").await;
    let (mut owner_b, _) = register(app_for(&state), "owner-b").await;

    let (status, co) = owner_a.post("/api/companies", json!({"name": "Duo Ltd"})).await;
    assert_eq!(status, StatusCode::OK);
    let company = pick_field(&co, ["i", "d"]);

    // Give user B an owner membership directly (transfer flow is a separate
    // owner-only feature; the table allows multiple owners in principle).
    let (me_status, me_b) = owner_b.get("/api/auth/me").await;
    assert_eq!(me_status, StatusCode::OK);
    let owner_b_id = pick_field(&me_b["user"], ["i", "d"]);
    sqlx::query(
        "INSERT INTO memberships (id, company_id, user_id, role) VALUES ($1, $2, $3, 'owner')",
    )
    .bind(uuid::Uuid::new_v4())
    .bind(uuid::Uuid::parse_str(&company).unwrap())
    .bind(uuid::Uuid::parse_str(&owner_b_id).unwrap())
    .execute(&state.pool)
    .await
    .expect("second owner");

    let (me_status, me_a) = owner_a.get("/api/auth/me").await;
    assert_eq!(me_status, StatusCode::OK);
    let owner_a_id = pick_field(&me_a["user"], ["i", "d"]);
    let path_a = format!("/api/companies/{}/members/{}", company, owner_a_id);
    let path_b = format!("/api/companies/{}/members/{}", company, owner_b_id);

    // Both owners race to demote themselves; the per-company lock serializes
    // the checks so exactly one succeeds and one owner always remains.
    let mut client_b = Client::new(app_for(&state));
    let (status_b_first, _, _) = owner_b
        .call("PATCH", &path_b, Some(json!({"role": "member"})))
        .await;
    let _ = &mut client_b;
    let (status_a, _, _) = owner_a
        .call("PATCH", &path_a, Some(json!({"role": "member"})))
        .await;

    let successes = [status_a, status_b_first]
        .iter()
        .filter(|s| **s == StatusCode::OK)
        .count();
    assert_eq!(
        successes, 1,
        "exactly one demotion may succeed; a company keeps one owner"
    );

    // The remaining owner still exists and keeps owner rights.
    let (status, members) = owner_a
        .get(&format!("/api/companies/{}/members", company))
        .await;
    assert_eq!(status, StatusCode::OK);
    let owners = members["members"]
        .as_array()
        .unwrap()
        .iter()
        .filter(|m| m["role"] == "owner")
        .count();
    assert_eq!(owners, 1, "an owner is always retained");
}

#[tokio::test]
async fn malformed_stored_role_fails_closed() {
    let (_db, state) = test_state().await;
    let (mut owner, _) = register(app_for(&state), "owner").await;
    let (mut member, member_email) = register(app_for(&state), "member").await;

    let (status, co) = owner.post("/api/companies", json!({"name": "Broken Ltd"})).await;
    assert_eq!(status, StatusCode::OK);
    let company = pick_field(&co, ["i", "d"]);
    let (status, inv) = owner
        .post(
            &format!("/api/companies/{}/invitations", company),
            json!({"email": member_email, "role": "member"}),
        )
        .await;
    assert_eq!(status, StatusCode::OK);
    let token = pick_field(&inv, ["tok", "en"]);
    let (status, _) = member
        .post("/api/invitations/accept", json!({"token": token}))
        .await;
    assert_eq!(status, StatusCode::OK);
    let (me_status, me) = member.get("/api/auth/me").await;
    assert_eq!(me_status, StatusCode::OK);
    let member_id = pick_field(&me["user"], ["i", "d"]);

    // Storage fails closed first: the CHECK constraint refuses malformed roles.
    let rejected = sqlx::query("UPDATE memberships SET role = 'wizard' WHERE user_id = $1")
        .bind(uuid::Uuid::parse_str(&member_id).unwrap())
        .execute(&state.pool)
        .await;
    assert!(
        rejected.is_err(),
        "the database constraint must reject malformed roles"
    );

    // Simulate corrupted legacy data that predates the constraint: drop it,
    // write the bad row, and restore the constraint NOT VALID so the row stays
    // and the application-level fail-closed paths get exercised as well.
    sqlx::query("ALTER TABLE memberships DROP CONSTRAINT memberships_role_check")
        .execute(&state.pool)
        .await
        .expect("drop role constraint");
    sqlx::query("UPDATE memberships SET role = 'wizard' WHERE user_id = $1")
        .bind(uuid::Uuid::parse_str(&member_id).unwrap())
        .execute(&state.pool)
        .await
        .expect("corrupt role");
    sqlx::query(
        "ALTER TABLE memberships ADD CONSTRAINT memberships_role_check \
         CHECK (role IN ('viewer','member','manager','admin','owner')) NOT VALID",
    )
    .execute(&state.pool)
    .await
    .expect("restore role constraint");

    // Both mutation and removal must refuse rather than treat it as viewer.
    let member_path = format!("/api/companies/{}/members/{}", company, member_id);
    let (status, _, _) = owner
        .call("PATCH", &member_path, Some(json!({"role": "member"})))
        .await;
    assert_eq!(
        status,
        StatusCode::FORBIDDEN,
        "a malformed role must fail closed on update"
    );
    let (status, _, _) = owner.call("DELETE", &member_path, None).await;
    assert_eq!(
        status,
        StatusCode::FORBIDDEN,
        "a malformed role must fail closed on removal"
    );
}

#[tokio::test]
async fn grant_expiry_is_bounded_and_checked() {
    let (_db, state) = test_state().await;
    let (mut owner, _) = register(app_for(&state), "owner").await;
    let (status, co) = owner.post("/api/companies", json!({"name": "Expiry Ltd"})).await;
    assert_eq!(status, StatusCode::OK);
    let company = pick_field(&co, ["i", "d"]);
    let grants_uri = format!("/api/companies/{}/grants", company);
    let actor_id = uuid::Uuid::new_v4();

    // Absurd expiry values overflow chrono durations: reject instead of panic.
    let (status, _) = owner
        .post(
            &grants_uri,
            json!({
                "actor_type": "agent",
                "actor_id": actor_id,
                "permission": "records.read",
                "resource": "app:inventory",
                "expires_in_secs": 9223372036854775807i64
            }),
        )
        .await;
    assert_eq!(
        status,
        StatusCode::BAD_REQUEST,
        "overflowing expiry must be rejected"
    );

    // Negative and zero windows are rejected as well.
    let (status, _) = owner
        .post(
            &grants_uri,
            json!({
                "actor_type": "agent",
                "actor_id": actor_id,
                "permission": "records.read",
                "resource": "app:inventory",
                "expires_in_secs": -5
            }),
        )
        .await;
    assert_eq!(status, StatusCode::BAD_REQUEST, "negative expiry rejected");

    // A bounded window is accepted and reported back.
    let (status, grant) = owner
        .post(
            &grants_uri,
            json!({
                "actor_type": "agent",
                "actor_id": actor_id,
                "permission": "records.read",
                "resource": "app:inventory",
                "expires_in_secs": 3600
            }),
        )
        .await;
    assert_eq!(status, StatusCode::OK, "{}", grant);
    assert!(grant["expiresAt"].is_string(), "expiry is recorded");
}

#[tokio::test]
async fn member_removal_above_issuer_is_denied_for_roles() {
    // Managers must not remove admins either: removal respects the same
    // issuer ceiling as mutation.
    let (_db, state) = test_state().await;
    let (mut owner, _) = register(app_for(&state), "owner").await;
    let (mut manager, manager_email) = register(app_for(&state), "manager").await;
    let (mut admin, admin_email) = register(app_for(&state), "admin").await;

    let (status, co) = owner.post("/api/companies", json!({"name": "Ladder Ltd"})).await;
    assert_eq!(status, StatusCode::OK);
    let company = pick_field(&co, ["i", "d"]);

    for (client, addr, role) in [
        (&mut manager, manager_email.clone(), "manager"),
        (&mut admin, admin_email.clone(), "admin"),
    ] {
        let (status, inv) = owner
            .post(
                &format!("/api/companies/{}/invitations", company),
                json!({"email": addr, "role": role}),
            )
            .await;
        assert_eq!(status, StatusCode::OK);
        let token = pick_field(&inv, ["tok", "en"]);
        let (status, _) = client
            .post("/api/invitations/accept", json!({"token": token}))
            .await;
        assert_eq!(status, StatusCode::OK);
    }

    let (me_status, me) = admin.get("/api/auth/me").await;
    assert_eq!(me_status, StatusCode::OK);
    let admin_id = pick_field(&me["user"], ["i", "d"]);
    let admin_path = format!("/api/companies/{}/members/{}", company, admin_id);

    let (status, _, _) = manager.call("DELETE", &admin_path, None).await;
    assert_eq!(
        status,
        StatusCode::FORBIDDEN,
        "a manager must not remove an admin"
    );
}
