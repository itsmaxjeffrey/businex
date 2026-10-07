//! Row level security tests with the real least-privilege runtime role.
//!
//! Every test gets its own disposable database (TestDb) and connects as the
//! provisioned businex_app / businex_service roles: businex_app is an ordinary
//! non-superuser and RLS must contain it; businex_service is the worker role.
//! The API runtime must not be superuser and must not be able to assume the
//! service role.

use businex_db::{
    begin_company_tx, begin_company_tx_on, current_role_attributes, ensure_api_role_safe,
    ensure_worker_role_safe, CompanyContext, TestDb, TEST_APP_PASSWORD, TEST_SERVICE_PASSWORD,
};
use sqlx::{Connection, Row};
use uuid::Uuid;

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
    let db = TestDb::new().await;
    let admin = &db.pool;

    // Runtime role: ordinary login role subject to RLS.
    let app = db.role_pool("businex_app", TEST_APP_PASSWORD).await;
    let attrs = current_role_attributes(&app).await.expect("attrs");
    assert_eq!(attrs.role, "businex_app");
    assert!(!attrs.superuser, "runtime role must not be superuser");
    assert!(!attrs.bypassrls, "runtime role must be subject to RLS");
    assert!(!attrs.member_of_service, "runtime role must not join businex_service");
    ensure_api_role_safe(&attrs).expect("api role policy");

    // Service role: workers span tenants by design, but never superuser.
    let service = db.role_pool("businex_service", TEST_SERVICE_PASSWORD).await;
    let sattrs = current_role_attributes(&service).await.expect("attrs");
    assert_eq!(sattrs.role, "businex_service");
    assert!(!sattrs.superuser, "service role must not be superuser");
    assert!(sattrs.bypassrls, "service role must bypass RLS as documented");
    ensure_worker_role_safe(&sattrs).expect("worker role policy");

    // The catalog must show exactly these attributes (fail closed claim).
    let rows = sqlx::query(
        "SELECT rolname, rolsuper, rolbypassrls FROM pg_roles WHERE rolname IN ('businex_app','businex_service')",
    )
    .fetch_all(admin)
    .await
    .expect("role lookup");
    let attr = |name: &str| {
        rows.iter()
            .find(|r| r.get::<String, _>("rolname") == name)
            .unwrap_or_else(|| panic!("role {} must exist", name))
    };
    assert!(!attr("businex_app").get::<bool, _>("rolbypassrls"));
    assert!(attr("businex_service").get::<bool, _>("rolbypassrls"));

    // Policy checks fail closed on violations.
    let mut bad = sattrs.clone();
    bad.superuser = true;
    assert!(ensure_worker_role_safe(&bad).is_err(), "superuser worker must be refused");
    let mut bad_api = attrs.clone();
    bad_api.bypassrls = true;
    assert!(ensure_api_role_safe(&bad_api).is_err(), "BYPASSRLS api role must be refused");
    let mut bad_member = attrs.clone();
    bad_member.member_of_service = true;
    assert!(
        ensure_api_role_safe(&bad_member).is_err(),
        "api role in businex_service must be refused"
    );
}

#[tokio::test]
async fn runtime_role_cannot_assume_service_role() {
    let db = TestDb::new().await;
    let app = db.role_pool("businex_app", TEST_APP_PASSWORD).await;
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
    let db = TestDb::new().await;
    let admin = &db.pool;
    let app = db.role_pool("businex_app", TEST_APP_PASSWORD).await;
    let company_a = new_company(admin, "RLS Company A").await;
    let company_b = new_company(admin, "RLS Company B").await;

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
    .execute(admin)
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
    let db = TestDb::new().await;
    let admin = &db.pool;
    let service = db.role_pool("businex_service", TEST_SERVICE_PASSWORD).await;
    let company_a = new_company(admin, "Service Co A").await;
    let company_b = new_company(admin, "Service Co B").await;

    // Workers legitimately see every tenant's jobs (documented BYPASSRLS).
    let n = sqlx::query("SELECT count(*) AS n FROM companies WHERE id = ANY($1)")
        .bind(vec![company_a, company_b])
        .fetch_one(&service)
        .await
        .expect("service read");
    assert_eq!(n.get::<i64, _>("n"), 2, "service role spans tenants by design");
}
