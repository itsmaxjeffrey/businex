//! Integration tests for the durable queue against a real PostgreSQL.
//!
//! Set BUSINEX_TEST_DATABASE_URL (or DATABASE_URL) to a disposable database;
//! the tests create and drop their own rows but share one schema. Every test
//! uses a unique job kind and claims only that kind, so parallel tests cannot
//! take each other's jobs.

use businex_db::TestDb;
use businex_queue::{
    cancel, claim, complete, create_schedule, enqueue, fail, fire_due_schedules, get_job,
    heartbeat, is_canceled, pending_effects, record_effect, reclaim_expired, resolve_effect,
    EnqueueOutcome, EffectOutcome, JobState, NewJob, NewSchedule, QueueError,
};
use chrono::{Duration, Utc};
use serde_json::json;
use sqlx::PgPool;
use uuid::Uuid;

/// Unique job kind per test run: claims stay inside one test even if
/// databases were shared.
fn uniq(prefix: &str) -> String {
    format!("{}-{}", prefix, Uuid::new_v4())
}

/// A disposable database per test. Requires BUSINEX_TEST_DATABASE_URL (an
/// explicitly disposable development server); there is no DATABASE_URL
/// fallback because that variable may point at production.
async fn test_pool() -> (TestDb, PgPool) {
    let db = TestDb::new().await;
    let pool = db.pool.clone();
    (db, pool)
}

async fn new_company(pool: &PgPool) -> Uuid {
    let id = Uuid::new_v4();
    sqlx::query("INSERT INTO companies (id, name, slug) VALUES ($1, $2, $3)")
        .bind(id)
        .bind(format!("Test Co {}", id))
        .bind(format!("test-co-{}", id))
        .execute(pool)
        .await
        .expect("insert company");
    id
}

#[tokio::test]
async fn enqueue_and_claim_roundtrip() {
    let (_db, pool) = test_pool().await;
    let company = new_company(&pool).await;
    let kind = uniq("echo");
    let out = enqueue(
        &pool,
        NewJob::new(Some(company), kind.clone(), json!({"hello": "world"})),
    )
    .await
    .expect("enqueue");
    let job = match out {
        EnqueueOutcome::Enqueued(j) => j,
        EnqueueOutcome::Duplicate(_) => panic!("first enqueue must be new"),
    };
    assert_eq!(job.state(), JobState::Queued);

    let claimed = claim(&pool, "worker-1", &[kind.clone()], std::time::Duration::from_secs(30))
        .await
        .expect("claim")
        .expect("a claimable job");
    assert_eq!(claimed.id, job.id);
    assert_eq!(claimed.state(), JobState::Leased);
    assert_eq!(claimed.attempts, 1);
    assert!(claimed.lease_token.is_some(), "claims carry a fencing token");

    let done = complete(&pool, &claimed, json!({"ok": true}))
        .await
        .expect("complete");
    assert_eq!(done.state(), JobState::Succeeded);
    assert_eq!(done.result, Some(json!({"ok": true})));
}

#[tokio::test]
async fn duplicate_enqueue_is_prevented_by_idempotency_key() {
    let (_db, pool) = test_pool().await;
    let company = new_company(&pool).await;
    let kind = uniq("sync");
    let first = enqueue(
        &pool,
        NewJob::new(Some(company), kind.clone(), json!({})).with_idempotency_key("import-42"),
    )
    .await
    .expect("enqueue");
    let second = enqueue(
        &pool,
        NewJob::new(Some(company), kind.clone(), json!({})).with_idempotency_key("import-42"),
    )
    .await
    .expect("enqueue");
    let id1 = match first {
        EnqueueOutcome::Enqueued(j) => j.id,
        _ => panic!("first must be new"),
    };
    let id2 = match second {
        EnqueueOutcome::Duplicate(j) => j.id,
        EnqueueOutcome::Enqueued(_) => panic!("second enqueue must be a duplicate"),
    };
    assert_eq!(id1, id2);
}

#[tokio::test]
async fn expired_lease_is_reclaimed_after_worker_death() {
    let (_db, pool) = test_pool().await;
    let company = new_company(&pool).await;
    let kind = uniq("long");
    let job = match enqueue(&pool, NewJob::new(Some(company), kind.clone(), json!({})))
        .await
        .expect("enqueue")
    {
        EnqueueOutcome::Enqueued(j) => j,
        _ => panic!("expected new job"),
    };

    // Worker claims with a very short lease, then "dies" without completing.
    let claimed = claim(&pool, "dead-worker", &[kind.clone()], std::time::Duration::from_millis(100))
        .await
        .expect("claim")
        .expect("claimable");
    assert_eq!(claimed.id, job.id);

    tokio::time::sleep(std::time::Duration::from_millis(300)).await;
    let reclaimed = reclaim_expired(&pool).await.expect("reclaim");
    assert!(
        reclaimed.iter().any(|j| j.id == job.id),
        "expired lease must return the job to the queue"
    );

    // Another worker picks it up: the work survives a worker restart.
    let again = claim(&pool, "fresh-worker", &[kind.clone()], std::time::Duration::from_secs(30))
        .await
        .expect("claim")
        .expect("reclaimable job");
    assert_eq!(again.id, job.id);
    assert_eq!(again.attempts, 2);
}

#[tokio::test]
async fn stale_attempt_with_same_worker_id_is_fenced() {
    let (_db, pool) = test_pool().await;
    let company = new_company(&pool).await;
    let kind = uniq("fence");
    let job = match enqueue(&pool, NewJob::new(Some(company), kind.clone(), json!({})))
        .await
        .expect("enqueue")
    {
        EnqueueOutcome::Enqueued(j) => j,
        _ => panic!("expected new job"),
    };

    // Attempt 1 by worker "w": lease expires, job is reclaimed and re-claimed
    // by the same worker id (attempt 2). The stale attempt must be fenced out
    // even though its worker id matches the current lease owner.
    let attempt1 = claim(&pool, "w", &[kind.clone()], std::time::Duration::from_millis(100))
        .await
        .expect("claim")
        .expect("claimable");
    tokio::time::sleep(std::time::Duration::from_millis(300)).await;
    reclaim_expired(&pool).await.expect("reclaim");
    let attempt2 = claim(&pool, "w", &[kind.clone()], std::time::Duration::from_secs(30))
        .await
        .expect("claim")
        .expect("re-claimable");
    assert_eq!(attempt2.id, job.id);
    assert_ne!(attempt1.lease_token, attempt2.lease_token, "each claim is a distinct attempt");

    // Stale attempt 1 cannot complete or fail the job.
    assert!(matches!(
        complete(&pool, &attempt1, json!({"stale": true})).await,
        Err(QueueError::Fenced)
    ));
    assert!(matches!(
        fail(&pool, &attempt1, "stale error", std::time::Duration::from_secs(0)).await,
        Err(QueueError::Fenced)
    ));

    // The current attempt completes normally.
    let done = complete(&pool, &attempt2, json!({"fresh": true}))
        .await
        .expect("current attempt completes");
    assert_eq!(done.state(), JobState::Succeeded);
    assert_eq!(done.result, Some(json!({"fresh": true})));
}

#[tokio::test]
async fn expired_lease_cannot_complete_or_refresh() {
    let (_db, pool) = test_pool().await;
    let company = new_company(&pool).await;
    let kind = uniq("stale");
    enqueue(&pool, NewJob::new(Some(company), kind.clone(), json!({})))
        .await
        .expect("enqueue");

    let claimed = claim(&pool, "slow-worker", &[kind.clone()], std::time::Duration::from_millis(100))
        .await
        .expect("claim")
        .expect("claimable");
    tokio::time::sleep(std::time::Duration::from_millis(300)).await;

    // The lease has expired but nothing reclaimed it yet: the attempt can no
    // longer refresh or complete it.
    assert!(!heartbeat(&pool, &claimed, std::time::Duration::from_secs(30))
        .await
        .expect("heartbeat"));
    assert!(matches!(
        complete(&pool, &claimed, json!({})).await,
        Err(QueueError::Fenced)
    ));
}

#[tokio::test]
async fn retries_are_bounded_and_recorded() {
    let (_db, pool) = test_pool().await;
    let company = new_company(&pool).await;
    let kind = uniq("flaky");
    let job = match enqueue(
        &pool,
        NewJob::new(Some(company), kind.clone(), json!({})).with_max_attempts(2),
    )
    .await
    .expect("enqueue")
    {
        EnqueueOutcome::Enqueued(j) => j,
        _ => panic!("expected new job"),
    };

    for attempt in 1..=2 {
        let claimed = claim(&pool, "worker", &[kind.clone()], std::time::Duration::from_secs(30))
            .await
            .expect("claim")
            .expect("claimable");
        assert_eq!(claimed.id, job.id);
        let outcome = fail(
            &pool,
            &claimed,
            "provider timeout",
            std::time::Duration::from_secs(0),
        )
        .await
        .expect("fail");
        if attempt < 2 {
            assert_eq!(outcome.state(), JobState::Queued, "retry is scheduled");
        } else {
            assert_eq!(outcome.state(), JobState::Failed, "bounded retries end in failed");
        }
    }
    let final_job = get_job(&pool, job.id).await.expect("get").expect("row");
    assert_eq!(final_job.attempts, 2);
    assert_eq!(final_job.last_error.as_deref(), Some("provider timeout"));
}

#[tokio::test]
async fn cancellation_is_visible_and_truthful() {
    let (_db, pool) = test_pool().await;
    let company = new_company(&pool).await;
    let kind_q = uniq("queued-kind");
    let queued = match enqueue(&pool, NewJob::new(Some(company), kind_q.clone(), json!({})))
        .await
        .expect("enqueue")
    {
        EnqueueOutcome::Enqueued(j) => j,
        _ => panic!("expected new job"),
    };
    let canceled = cancel(&pool, queued.id).await.expect("cancel");
    assert_eq!(canceled.state(), JobState::Canceled);

    let kind_r = uniq("running-kind");
    let running = match enqueue(&pool, NewJob::new(Some(company), kind_r.clone(), json!({})))
        .await
        .expect("enqueue")
    {
        EnqueueOutcome::Enqueued(j) => j,
        _ => panic!("expected new job"),
    };
    let claimed = claim(&pool, "worker", &[kind_r.clone()], std::time::Duration::from_secs(30))
        .await
        .expect("claim")
        .expect("claimable");
    let flagged = cancel(&pool, running.id).await.expect("cancel");
    assert_eq!(flagged.state(), JobState::Leased, "leased job keeps running state");
    assert!(is_canceled(&pool, running.id).await.expect("check"));
    // The worker notices and finishes; the recorded outcome is canceled.
    let done = complete(&pool, &claimed, json!({"stopped": true}))
        .await
        .expect("complete");
    assert_eq!(done.state(), JobState::Canceled);
}

#[tokio::test]
async fn uncertain_external_outcomes_stay_reconcilable() {
    let (_db, pool) = test_pool().await;
    let company = new_company(&pool).await;
    let job = match enqueue(&pool, NewJob::new(Some(company), uniq("webhook"), json!({})))
        .await
        .expect("enqueue")
    {
        EnqueueOutcome::Enqueued(j) => j,
        _ => panic!("expected new job"),
    };

    // Before the external call the effect is recorded as pending.
    let effect = record_effect(&pool, job.id, "call:billing-api", "sha256:abc")
        .await
        .expect("record");
    assert_eq!(effect.outcome, "pending");

    // The worker dies here; a restarted worker finds the pending effect.
    let pending = pending_effects(&pool, job.id).await.expect("pending");
    assert_eq!(pending.len(), 1);
    assert_eq!(pending[0].effect_key, "call:billing-api");

    // Reconciliation confirms the external side happened exactly once.
    let resolved = resolve_effect(
        &pool,
        job.id,
        "call:billing-api",
        EffectOutcome::Confirmed,
        json!({"response": 200}),
    )
    .await
    .expect("resolve");
    assert_eq!(resolved.outcome, "confirmed");
    assert!(pending_effects(&pool, job.id).await.expect("pending").is_empty());
}

#[tokio::test]
async fn schedules_fire_exactly_once() {
    let (_db, pool) = test_pool().await;
    let company = new_company(&pool).await;
    let schedule = create_schedule(
        &pool,
        NewSchedule {
            company_id: Some(company),
            kind: uniq("report"),
            payload: json!({"type": "daily"}),
            interval_seconds: Some(3600),
            cron: None,
        },
    )
    .await
    .expect("create");

    let fired = fire_due_schedules(&pool).await.expect("fire");
    assert_eq!(
        fired.iter().filter(|(s, _)| s.id == schedule.id).count(),
        1,
        "the schedule fires once"
    );
    let job_id = fired
        .iter()
        .find(|(s, _)| s.id == schedule.id)
        .expect("fired")
        .1
        .id;

    // Immediately firing again must not enqueue a duplicate of this schedule.
    let again = fire_due_schedules(&pool).await.expect("fire");
    assert!(
        again.iter().all(|(s, _)| s.id != schedule.id),
        "schedule advanced past its fire time"
    );

    // Even if the schedule somehow becomes due again for the same fire time,
    // the idempotency key prevents a second job.
    sqlx::query("UPDATE job_schedules SET next_run_at = $2 WHERE id = $1")
        .bind(schedule.id)
        .bind(schedule.next_run_at)
        .execute(&pool)
        .await
        .expect("rewind");
    let replay = fire_due_schedules(&pool).await.expect("fire");
    assert!(
        replay.iter().all(|(s, _)| s.id != schedule.id),
        "same fire time must not duplicate the job"
    );
    let still = get_job(&pool, job_id).await.expect("get").expect("row");
    assert_eq!(still.idempotency_key, Some(format!("schedule:{}:{}", schedule.id, schedule.next_run_at.to_rfc3339())));
}

#[tokio::test]
async fn cron_schedules_advance_to_the_next_match() {
    let (_db, pool) = test_pool().await;
    let company = new_company(&pool).await;
    let schedule = create_schedule(
        &pool,
        NewSchedule {
            company_id: Some(company),
            kind: uniq("cron-report"),
            payload: json!({}),
            interval_seconds: None,
            cron: Some("0 6 * * *".into()),
        },
    )
    .await
    .expect("create");
    assert!(schedule.next_run_at > Utc::now(), "cron schedules start in the future");
    assert!(schedule.next_run_at <= Utc::now() + Duration::hours(24));
}
