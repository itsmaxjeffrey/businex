//! Worker behavior against a real PostgreSQL: restart recovery, duplicate
//! prevention, bounded retries, cancellation and uncertain outcomes.

use async_trait::async_trait;
use businex_events::{Relay, RelayOptions};
use businex_queue::{enqueue, EnqueueOutcome, NewJob, JobState};
use businex_worker::{HandlerError, JobContext, JobHandler, Worker};
use serde_json::json;
use sqlx::PgPool;
use std::sync::atomic::{AtomicU32, Ordering};
use std::sync::{Arc, OnceLock};
use std::time::Duration;

fn database_url() -> String {
    std::env::var("BUSINEX_TEST_DATABASE_URL")
        .or_else(|_| std::env::var("DATABASE_URL"))
        .expect("set BUSINEX_TEST_DATABASE_URL to a disposable PostgreSQL database")
}

async fn test_pool() -> PgPool {
    static MIGRATED: OnceLock<()> = OnceLock::new();
    let pool = businex_db::connect(&database_url(), 8).await.expect("connect");
    if MIGRATED.get().is_none() {
        businex_db::run_migrations(&pool).await.expect("migrate");
        let _ = MIGRATED.set(());
    }
    pool
}

async fn new_company(pool: &PgPool) -> uuid::Uuid {
    let id = uuid::Uuid::new_v4();
    sqlx::query("INSERT INTO companies (id, name, slug) VALUES ($1, $2, $3)")
        .bind(id)
        .bind(format!("Worker Co {}", id))
        .bind(format!("worker-co-{}", id))
        .execute(pool)
        .await
        .expect("insert company");
    id
}

struct CountingHandler {
    runs: Arc<AtomicU32>,
    behavior: Behavior,
}

#[derive(Clone, Copy)]
enum Behavior {
    Succeed,
    RetryOnce,
    FailFatal,
}

#[async_trait]
impl JobHandler for CountingHandler {
    async fn handle(&self, _ctx: &JobContext) -> businex_worker::HandlerResult {
        let n = self.runs.fetch_add(1, Ordering::SeqCst) + 1;
        match self.behavior {
            Behavior::Succeed => Ok(json!({"runs": n})),
            Behavior::RetryOnce if n == 1 => {
                Err(HandlerError::Retryable("transient provider error".into()))
            }
            Behavior::RetryOnce => Ok(json!({"runs": n})),
            Behavior::FailFatal => Err(HandlerError::Fatal("bad input".into())),
        }
    }
}

fn worker_with(pool: PgPool, kind: &str, handler: Arc<CountingHandler>) -> Worker {
    let relay = Relay::new(RelayOptions::default());
    let mut worker = Worker::new(pool, relay)
        .with_lease(Duration::from_secs(30))
        .with_poll_interval(Duration::from_millis(10));
    worker.register(kind, handler);
    worker
}

#[tokio::test]
async fn run_once_executes_handler_and_completes() {
    let pool = test_pool().await;
    let company = new_company(&pool).await;
    enqueue(&pool, NewJob::new(Some(company), "t1", json!({})))
        .await
        .expect("enqueue");

    let runs = Arc::new(AtomicU32::new(0));
    let worker = worker_with(
        pool.clone(),
        "t1",
        Arc::new(CountingHandler {
            runs: runs.clone(),
            behavior: Behavior::Succeed,
        }),
    );
    let report = worker.run_once().await.expect("cycle").expect("job ran");
    assert_eq!(report.outcome, "succeeded");
    assert_eq!(runs.load(Ordering::SeqCst), 1);

    let job = businex_queue::get_job(&pool, report.job_id).await.expect("get").expect("row");
    assert_eq!(job.state(), JobState::Succeeded);
    assert_eq!(job.result, Some(json!({"runs": 1})));

    // Nothing left: a second cycle finds no work.
    assert!(worker.run_once().await.expect("cycle").is_none());
}

#[tokio::test]
async fn duplicate_enqueue_never_runs_twice() {
    let pool = test_pool().await;
    let company = new_company(&pool).await;
    let key = format!("run-{}", uuid::Uuid::new_v4());
    for _ in 0..3 {
        enqueue(
            &pool,
            NewJob::new(Some(company), "t2", json!({})).with_idempotency_key(&key),
        )
        .await
        .expect("enqueue");
    }
    let runs = Arc::new(AtomicU32::new(0));
    let worker = worker_with(
        pool.clone(),
        "t2",
        Arc::new(CountingHandler {
            runs: runs.clone(),
            behavior: Behavior::Succeed,
        }),
    );
    assert!(worker.run_once().await.expect("cycle").is_some());
    assert!(worker.run_once().await.expect("cycle").is_none());
    assert!(worker.run_once().await.expect("cycle").is_none());
    assert_eq!(runs.load(Ordering::SeqCst), 1, "duplicate enqueue must run once");
}

#[tokio::test]
async fn crash_before_completion_is_recovered_exactly_once() {
    let pool = test_pool().await;
    let company = new_company(&pool).await;
    let job = match enqueue(&pool, NewJob::new(Some(company), "t3", json!({})))
        .await
        .expect("enqueue")
    {
        EnqueueOutcome::Enqueued(j) => j,
        _ => panic!("expected new job"),
    };

    // A worker claims the job and dies: no completion, lease expires.
    businex_queue::claim(&pool, "crashed-worker", &["t3".into()], Duration::from_millis(100))
        .await
        .expect("claim")
        .expect("claimable");
    tokio::time::sleep(Duration::from_millis(300)).await;

    // A restarted worker recovers the job and runs it exactly once.
    let runs = Arc::new(AtomicU32::new(0));
    let worker = worker_with(
        pool.clone(),
        "t3",
        Arc::new(CountingHandler {
            runs: runs.clone(),
            behavior: Behavior::Succeed,
        }),
    );
    let report = worker.run_once().await.expect("cycle").expect("recovered");
    assert_eq!(report.job_id, job.id);
    assert_eq!(report.outcome, "succeeded");
    assert_eq!(runs.load(Ordering::SeqCst), 1, "recovery must not duplicate execution");
    let done = businex_queue::get_job(&pool, job.id).await.expect("get").expect("row");
    assert_eq!(done.attempts, 2, "crashed attempt is counted");
}

#[tokio::test]
async fn retryable_errors_requeue_then_succeed() {
    let pool = test_pool().await;
    let company = new_company(&pool).await;
    enqueue(&pool, NewJob::new(Some(company), "t4", json!({})))
        .await
        .expect("enqueue");

    let runs = Arc::new(AtomicU32::new(0));
    let worker = worker_with(
        pool.clone(),
        "t4",
        Arc::new(CountingHandler {
            runs: runs.clone(),
            behavior: Behavior::RetryOnce,
        }),
    );
    let first = worker.run_once().await.expect("cycle").expect("first attempt");
    assert_eq!(first.outcome, "queued", "retryable failure requeues");

    // Give the backoff window a moment; backoff for attempt 1 is 2 seconds.
    tokio::time::sleep(Duration::from_millis(2100)).await;
    let second = worker.run_once().await.expect("cycle").expect("second attempt");
    assert_eq!(second.outcome, "succeeded");
    assert_eq!(runs.load(Ordering::SeqCst), 2);
    let job = businex_queue::get_job(&pool, second.job_id).await.expect("get").expect("row");
    assert_eq!(job.attempts, 2);
}

#[tokio::test]
async fn fatal_errors_fail_without_retry() {
    let pool = test_pool().await;
    let company = new_company(&pool).await;
    enqueue(&pool, NewJob::new(Some(company), "t5", json!({})))
        .await
        .expect("enqueue");

    let runs = Arc::new(AtomicU32::new(0));
    let worker = worker_with(
        pool.clone(),
        "t5",
        Arc::new(CountingHandler {
            runs: runs.clone(),
            behavior: Behavior::FailFatal,
        }),
    );
    let report = worker.run_once().await.expect("cycle").expect("attempt");
    assert_eq!(report.outcome, "failed");
    assert!(worker.run_once().await.expect("cycle").is_none(), "no retry for fatal errors");
    assert_eq!(runs.load(Ordering::SeqCst), 1);
}

#[tokio::test]
async fn canceled_jobs_never_execute() {
    let pool = test_pool().await;
    let company = new_company(&pool).await;
    let job = match enqueue(&pool, NewJob::new(Some(company), "t6", json!({})))
        .await
        .expect("enqueue")
    {
        EnqueueOutcome::Enqueued(j) => j,
        _ => panic!("expected new job"),
    };
    businex_queue::cancel(&pool, job.id).await.expect("cancel");

    let runs = Arc::new(AtomicU32::new(0));
    let worker = worker_with(
        pool.clone(),
        "t6",
        Arc::new(CountingHandler {
            runs: runs.clone(),
            behavior: Behavior::Succeed,
        }),
    );
    assert!(worker.run_once().await.expect("cycle").is_none(), "canceled job is not claimed");
    assert_eq!(runs.load(Ordering::SeqCst), 0);
}

struct Reconciler {
    runs: Arc<AtomicU32>,
}

#[async_trait]
impl JobHandler for Reconciler {
    async fn handle(&self, _ctx: &JobContext) -> businex_worker::HandlerResult {
        let n = self.runs.fetch_add(1, Ordering::SeqCst) + 1;
        Ok(json!({"runs": n}))
    }

    async fn reconcile(
        &self,
        ctx: &JobContext,
        effects: &[businex_queue::JobEffect],
    ) -> Result<(), businex_worker::HandlerError> {
        // The handler checks the external system and confirms the effect.
        for effect in effects {
            ctx.resolve_effect(
                &effect.effect_key,
                businex_queue::EffectOutcome::Confirmed,
                json!({"checked": true}),
            )
            .await
            .expect("resolve");
        }
        Ok(())
    }
}

#[tokio::test]
async fn pending_external_effects_block_blind_retry() {
    let pool = test_pool().await;
    let company = new_company(&pool).await;
    let job = match enqueue(&pool, NewJob::new(Some(company), "t7", json!({})))
        .await
        .expect("enqueue")
    {
        EnqueueOutcome::Enqueued(j) => j,
        _ => panic!("expected new job"),
    };

    // A crashed attempt already performed an external call: the effect is
    // recorded but unresolved.
    businex_queue::claim(&pool, "crashed", &["t7".into()], Duration::from_secs(30))
        .await
        .expect("claim");
    businex_queue::record_effect(&pool, job.id, "call:payments", "sha256:x")
        .await
        .expect("record");
    // Crash: nothing resolves the effect; lease expires.
    sqlx::query("UPDATE jobs SET lease_expires_at = now() - interval '1 second' WHERE id = $1")
        .bind(job.id)
        .execute(&pool)
        .await
        .expect("expire lease");

    // Without a reconciler the worker refuses to rerun the job blindly.
    let runs = Arc::new(AtomicU32::new(0));
    let worker = worker_with(
        pool.clone(),
        "t7",
        Arc::new(CountingHandler {
            runs: runs.clone(),
            behavior: Behavior::Succeed,
        }),
    );
    let report = worker.run_once().await.expect("cycle").expect("recovered job");
    assert_eq!(report.outcome, "failed", "blind retry must be refused");
    assert_eq!(runs.load(Ordering::SeqCst), 0);

    // A handler with reconciliation resolves the effect and then runs.
    let job2 = match enqueue(&pool, NewJob::new(Some(company), "t8", json!({})))
        .await
        .expect("enqueue")
    {
        EnqueueOutcome::Enqueued(j) => j,
        _ => panic!("expected new job"),
    };
    businex_queue::claim(&pool, "crashed-2", &["t8".into()], Duration::from_secs(30))
        .await
        .expect("claim");
    businex_queue::record_effect(&pool, job2.id, "call:payments", "sha256:y")
        .await
        .expect("record");
    sqlx::query("UPDATE jobs SET lease_expires_at = now() - interval '1 second' WHERE id = $1")
        .bind(job2.id)
        .execute(&pool)
        .await
        .expect("expire lease");

    let runs2 = Arc::new(AtomicU32::new(0));
    let relay = Relay::new(RelayOptions::default());
    let mut reconciling = Worker::new(pool.clone(), relay)
        .with_lease(Duration::from_secs(30))
        .with_poll_interval(Duration::from_millis(10));
    reconciling.register("t8", Arc::new(Reconciler { runs: runs2.clone() }));
    let report2 = reconciling.run_once().await.expect("cycle").expect("reconciled job");
    assert_eq!(report2.outcome, "succeeded");
    assert_eq!(runs2.load(Ordering::SeqCst), 1);
}
