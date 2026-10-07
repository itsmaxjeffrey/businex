//! Tenant scoping of queue operations through the real runtime role.
//!
//! Cancellation and job reads run inside the caller's company transaction:
//! row level security must confine them to the tenant. Cross-tenant job ids
//! are invisible and uncancellable. Each test uses its own disposable
//! database.

use businex_db::{begin_company_tx, CompanyContext, TestDb, TEST_APP_PASSWORD};
use businex_queue::{cancel_scoped, enqueue, get_job_scoped, EnqueueOutcome, NewJob};
use serde_json::json;
use uuid::Uuid;

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
    let db = TestDb::new().await;
    let admin = &db.pool;
    let app = db.role_pool("businex_app", TEST_APP_PASSWORD).await;
    let company_a = new_company(admin, "Cancel Co A").await;
    let company_b = new_company(admin, "Cancel Co B").await;

    let job_a = match enqueue(admin, NewJob::new(Some(company_a), "demo", json!({})))
        .await
        .expect("enqueue a")
    {
        EnqueueOutcome::Enqueued(j) => j,
        _ => panic!("expected new job"),
    };
    let job_b = match enqueue(admin, NewJob::new(Some(company_b), "demo", json!({})))
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
