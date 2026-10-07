//! Row level security tests with the real least-privilege runtime role.
//!
//! These tests provision login passwords for the businex_app and
//! businex_service roles over the admin connection (test-only credentials),
//! then connect as those roles: businex_app is an ordinary non-superuser and
//! RLS must contain it; businex_service is the worker role. The API runtime
//! must not be superuser and must not be able to assume the service role.

use businex_db::{begin_company_tx, begin_company_tx_on, CompanyContext};
use sqlx::{Connection, Row};
use uuid::Uuid;

const APP_PASSWORD: &str = "businex-app-test-pw";
const SERVICE_PASSWORD: &str = "businex-service-test-pw";

fn admin_url() -> String {
    std::env::var("BUSINEX_TEST_DATABASE_URL")
        .or_else(|_| std::env::var("DATABASE_URL"))
        .expect("set BUSINEX_TEST_DATABASE_URL to a disposable PostgreSQL database")
}

/// Swap the user info of a postgres URL for another role.
fn role_url(admin: &str, user: &str, password: &str) -> String {
    let scheme_end = admin.find("://").expect("scheme") + 3;
    let at = admin.rfind('@').expect("user info");
    format!(
        "{}{}:{}@{}",
        &admin[..scheme_end],
        user,
        password,
        &admin[at + 1..]
    )
}

static SETUP: tokio::sync::OnceCell<()> = tokio::sync::OnceCell::const_new();

async fn setup() {
    SETUP
        .get_or_init(|| async {
            let admin = businex_db::connect(&admin_url(), 4).await.expect("admin connect");
            businex_db::run_migrations(&admin).await.expect("migrate");
            // Provision test-only login passwords. Production provisioning does
            // this out of band; passwords never live in the repository. This
            // runs exactly once: parallel ALTER ROLE on pg_authid conflicts.
            for (role, password) in [
                ("businex_app", APP_PASSWORD),
                ("businex_service", SERVICE_PASSWORD),
            ] {
                sqlx::query(&format!(
                    "ALTER ROLE {} LOGIN PASSWORD '{}'",
                    role, password
                ))
                .execute(&admin)
                .await
                .expect("provision role login");
            }
        })
        .await;
}

async fn app_pool() -> sqlx::PgPool {
    setup().await;
    businex_db::connect(&role_url(&admin_url(), "businex_app", APP_PASSWORD), 4)
        .await
        .expect("app role connect")
}

async fn service_pool() -> sqlx::PgPool {
    setup().await;
    businex_db::connect(
        &role_url(&admin_url(), "businex_service", SERVICE_PASSWORD),
        4,
    )
    .await
    .expect("service role connect")
}

async fn new_company(pool: &sqlx::PgPool, name: &str) -> Uuid {
    let id = Uuid::new_v4();
    sqlx::query("INSERT INTO companies (id, name, slug) VALUES ($1, $2, $3)")
        .bind(id)
        .bind(name)
        .bind(format!("rls-{}", id))
        .execute(pool)
        .await
        .expect("insert company");
    id
}

#[tokio::test]
async fn role_attributes_match_documented_behavior() {
    setup().await;
    let admin = businex_db::connect(&admin_url(), 2).await.expect("admin");
    let rows = sqlx::query(
        "SELECT rolname, rolsuper, rolbypassrls FROM pg_roles WHERE rolname IN ('businex_app','businex_service')",
    )
    .fetch_all(&admin)
    .await
    .expect("role lookup");
    let attr = |name: &str| {
        rows.iter()
            .find(|r| r.get::<String, _>("rolname") == name)
            .unwrap_or_else(|| panic!("role {} must exist", name))
    };
    let app = attr("businex_app");
    assert!(!app.get::<bool, _>("rolsuper"), "runtime role must not be superuser");
    assert!(!app.get::<bool, _>("rolbypassrls"), "runtime role must be subject to RLS");
    let svc = attr("businex_service");
    assert!(!svc.get::<bool, _>("rolsuper"), "service role must not be superuser");
    assert!(
        svc.get::<bool, _>("rolbypassrls"),
        "service role must bypass RLS as documented (workers process all tenants)"
    );

    // The runtime role must not be able to assume the service role.
    let members = sqlx::query(
        "SELECT count(*) AS n FROM pg_auth_members m
         JOIN pg_roles r ON r.oid = m.roleid
         JOIN pg_roles g ON g.oid = m.member
         WHERE g.rolname = 'businex_app' AND r.rolname = 'businex_service'",
    )
    .fetch_one(&admin)
    .await
    .expect("membership lookup");
    assert_eq!(
        members.get::<i64, _>("n"),
        0,
        "businex_app must not be a member of businex_service"
    );
}

#[tokio::test]
async fn runtime_role_cannot_assume_service_role() {
    let app = app_pool().await;
    let mut conn = app.acquire().await.expect("conn");
    let mut tx = conn.begin().await.expect("tx");
    let result = sqlx::query("SET LOCAL ROLE businex_service")
        .execute(&mut *tx)
        .await;
    assert!(
        result.is_err(),
        "the runtime role must not be able to SET ROLE businex_service"
    );
    tx.rollback().await.expect("rollback");
}

#[tokio::test]
async fn tenant_isolation_for_read_insert_update_delete() {
    let admin = businex_db::connect(&admin_url(), 4).await.expect("admin");
    let app = app_pool().await;
    let company_a = new_company(&admin, "RLS Company A").await;
    let company_b = new_company(&admin, "RLS Company B").await;

    // Reads: each company sees only itself.
    let mut tx_a = begin_company_tx(&app, CompanyContext::new(company_a))
        .await
        .expect("tx a");
    let seen: Vec<Uuid> = sqlx::query("SELECT id FROM companies")
        .fetch_all(&mut *tx_a)
        .await
        .expect("select companies")
        .iter()
        .map(|r| r.get::<Uuid, _>("id"))
        .collect();
    assert_eq!(seen, vec![company_a], "company A must only see itself");
    tx_a.commit().await.expect("commit read");

    // Insert: a row for the wrong company is rejected by the policy. A rejected
    // statement aborts its transaction, so this check runs in its own one.
    let mut tx_f = begin_company_tx(&app, CompanyContext::new(company_a))
        .await
        .expect("tx f");
    let foreign = sqlx::query(
        "INSERT INTO audit_log (id, company_id, action, entity_type) VALUES ($1, $2, 'test', 'x')",
    )
    .bind(Uuid::new_v4())
    .bind(company_b)
    .execute(&mut *tx_f)
    .await;
    assert!(foreign.is_err(), "inserting another tenant's row must be rejected");
    tx_f.rollback().await.expect("rollback rejected insert");

    // Insert own row works.
    let mut tx_a = begin_company_tx(&app, CompanyContext::new(company_a))
        .await
        .expect("tx a2");
    let audit_id = Uuid::new_v4();
    sqlx::query(
        "INSERT INTO audit_log (id, company_id, action, entity_type) VALUES ($1, $2, 'test', 'x')",
    )
    .bind(audit_id)
    .bind(company_a)
    .execute(&mut *tx_a)
    .await
    .expect("own insert");
    tx_a.commit().await.expect("commit a");

    // Prepare a company B row via admin (service provisioning path).
    let audit_b = Uuid::new_v4();
    sqlx::query(
        "INSERT INTO audit_log (id, company_id, action, entity_type) VALUES ($1, $2, 'test', 'y')",
    )
    .bind(audit_b)
    .bind(company_b)
    .execute(&admin)
    .await
    .expect("admin insert b");

    // Update and delete: company A cannot touch company B's row.
    let mut tx = begin_company_tx(&app, CompanyContext::new(company_a))
        .await
        .expect("tx");
    let updated = sqlx::query("UPDATE audit_log SET action = 'hacked' WHERE id = $1")
        .bind(audit_b)
        .execute(&mut *tx)
        .await
        .expect("update runs");
    assert_eq!(updated.rows_affected(), 0, "foreign update must match no rows");
    let deleted = sqlx::query("DELETE FROM audit_log WHERE id = $1")
        .bind(audit_b)
        .execute(&mut *tx)
        .await
        .expect("delete runs");
    assert_eq!(deleted.rows_affected(), 0, "foreign delete must match no rows");

    // Own rows are writable.
    let own = sqlx::query("UPDATE audit_log SET action = 'updated' WHERE id = $1")
        .bind(audit_id)
        .execute(&mut *tx)
        .await
        .expect("own update");
    assert_eq!(own.rows_affected(), 1);
    tx.commit().await.expect("commit");

    // Pooled tenant-context reuse: the same pooled connection must not leak
    // one company's context into the next transaction.
    let mut conn = app.acquire().await.expect("conn");
    let mut tx = begin_company_tx_on(&mut conn, CompanyContext::new(company_a))
        .await
        .expect("tx on conn");
    let n = sqlx::query("SELECT count(*) AS n FROM companies")
        .fetch_one(&mut *tx)
        .await
        .expect("count");
    assert_eq!(n.get::<i64, _>("n"), 1);
    tx.commit().await.expect("commit");

    let mut tx = begin_company_tx_on(&mut conn, CompanyContext::new(company_b))
        .await
        .expect("tx on conn");
    let rows = sqlx::query("SELECT id FROM companies")
        .fetch_all(&mut *tx)
        .await
        .expect("select");
    assert_eq!(rows.len(), 1);
    assert_eq!(rows[0].get::<Uuid, _>("id"), company_b);
    tx.commit().await.expect("commit");

    // Without a company context nothing is visible at all.
    let mut tx = conn.begin().await.expect("tx");
    let n = sqlx::query("SELECT count(*) AS n FROM companies")
        .fetch_one(&mut *tx)
        .await
        .expect("count");
    assert_eq!(n.get::<i64, _>("n"), 0, "no context must deny all tenant rows");
    tx.rollback().await.expect("rollback");
}

#[tokio::test]
async fn service_role_processes_all_tenants() {
    let admin = businex_db::connect(&admin_url(), 4).await.expect("admin");
    let service = service_pool().await;
    let company_a = new_company(&admin, "Service Co A").await;
    let company_b = new_company(&admin, "Service Co B").await;

    // Workers legitimately see every tenant's jobs (documented BYPASSRLS).
    let n = sqlx::query("SELECT count(*) AS n FROM companies WHERE id = ANY($1)")
        .bind(vec![company_a, company_b])
        .fetch_one(&service)
        .await
        .expect("service read");
    assert_eq!(n.get::<i64, _>("n"), 2, "service role spans tenants by design");
}
