//! Tenant scoping of queue operations through the real runtime role.
//!
//! Cancellation and job reads run inside the caller's company transaction:
//! row level security must confine them to the tenant. Cross-tenant job ids
//! are invisible and uncancellable.

use businex_db::{begin_company_tx, CompanyContext};
use businex_queue::{cancel_scoped, enqueue, get_job_scoped, EnqueueOutcome, NewJob};
use serde_json::json;
use uuid::Uuid;

const APP_PASSWORD: &str = "businex-app-test-pw";

fn admin_url() -> String {
    std::env::var("BUSINEX_TEST_DATABASE_URL")
        .or_else(|_| std::env::var("DATABASE_URL"))
        .expect("set BUSINEX_TEST_DATABASE_URL to a disposable PostgreSQL database")
}

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

async fn setup_pools() -> (sqlx::PgPool, sqlx::PgPool) {
    let admin = businex_db::connect(&admin_url(), 4).await.expect("admin");
    // Exactly once: parallel ALTER ROLE on pg_authid conflicts otherwise.
    SETUP
        .get_or_init(|| async {
            businex_db::run_migrations(&admin).await.expect("migrate");
            sqlx::query(&format!(
                "ALTER ROLE businex_app LOGIN PASSWORD '{}'",
                APP_PASSWORD
            ))
            .execute(&admin)
            .await
            .expect("provision app login");
        })
        .await;
    let app = businex_db::connect(&role_url(&admin_url(), "businex_app", APP_PASSWORD), 4)
        .await
        .expect("app role connect");
    (admin, app)
}

async fn new_company(pool: &sqlx::PgPool, name: &str) -> Uuid {
    let id = Uuid::new_v4();
    sqlx::query("INSERT INTO companies (id, name, slug) VALUES ($1, $2, $3)")
        .bind(id)
        .bind(name)
        .bind(format!("qscope-{}", id))
        .execute(pool)
        .await
        .expect("insert company");
    id
}

#[tokio::test]
async fn queue_cancellation_is_tenant_scoped() {
    let (admin, app) = setup_pools().await;
    let company_a = new_company(&admin, "Cancel Co A").await;
    let company_b = new_company(&admin, "Cancel Co B").await;

    let job_a = match enqueue(&admin, NewJob::new(Some(company_a), "demo", json!({})))
        .await
        .expect("enqueue a")
    {
        EnqueueOutcome::Enqueued(j) => j,
        _ => panic!("expected new job"),
    };
    let job_b = match enqueue(&admin, NewJob::new(Some(company_b), "demo", json!({})))
        .await
        .expect("enqueue b")
    {
        EnqueueOutcome::Enqueued(j) => j,
        _ => panic!("expected new job"),
    };

    // Company A can cancel its own job inside its tenant transaction.
    let mut tx = begin_company_tx(&app, CompanyContext::new(company_a))
        .await
        .expect("tx");
    let canceled = cancel_scoped(&mut tx, job_a.id)
        .await
        .expect("cancel own");
    assert_eq!(canceled.state, "canceled");

    // Company A cannot even see, let alone cancel, company B's job.
    assert!(get_job_scoped(&mut tx, job_b.id)
        .await
        .expect("scoped get")
        .is_none());
    let foreign_cancel = cancel_scoped(&mut tx, job_b.id).await;
    assert!(
        foreign_cancel.is_err(),
        "cross-tenant cancellation must be denied"
    );
    tx.commit().await.expect("commit");

    // Company B's job is untouched and still visible to its own tenant.
    let mut tx = begin_company_tx(&app, CompanyContext::new(company_b))
        .await
        .expect("tx b");
    let still = get_job_scoped(&mut tx, job_b.id)
        .await
        .expect("get b")
        .expect("job b exists");
    assert_eq!(still.state, "queued");
    tx.rollback().await.expect("rollback");
}
