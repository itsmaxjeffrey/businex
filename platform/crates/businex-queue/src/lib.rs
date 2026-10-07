//! Durable job queue backed by PostgreSQL.
//!
//! Design rules:
//! - Durable work never depends on Redis Pub/Sub; PostgreSQL is the queue.
//! - A worker claims a job with a lease and a fencing token (FOR UPDATE SKIP
//!   LOCKED) and must heartbeat to keep it. Completion, failure and heartbeat
//!   must present the matching token while the lease is still fresh. An
//!   expired lease can no longer complete or refresh: if the worker died and
//!   another attempt started (even with the same worker id), the stale attempt
//!   is fenced out.
//! - Every enqueue can carry an idempotency key; a second enqueue with the
//!   same key returns the existing job instead of creating a duplicate.
//! - Retries are bounded by max_attempts with backoff; cancellation is visible
//!   to the running worker and recorded in the job history.
//! - External side effects are recorded before the call (pending) and resolved
//!   after it, so a crash mid-call leaves a reconcilable record. Pending
//!   effects are never replayed automatically.

pub mod cron;

pub use cron::{CronError, CronExpr};

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use sqlx::PgPool;
use uuid::Uuid;

#[derive(Debug, thiserror::Error)]
pub enum QueueError {
    #[error("database error")]
    Db(#[from] sqlx::Error),
    #[error("invalid schedule: {0}")]
    InvalidSchedule(String),
    #[error("stale or expired lease")]
    Fenced,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum JobState {
    Queued,
    Leased,
    Succeeded,
    Failed,
    Canceled,
}

impl JobState {
    pub fn as_str(self) -> &'static str {
        match self {
            JobState::Queued => "queued",
            JobState::Leased => "leased",
            JobState::Succeeded => "succeeded",
            JobState::Failed => "failed",
            JobState::Canceled => "canceled",
        }
    }

    pub fn parse(s: &str) -> Option<JobState> {
        match s {
            "queued" => Some(JobState::Queued),
            "leased" => Some(JobState::Leased),
            "succeeded" => Some(JobState::Succeeded),
            "failed" => Some(JobState::Failed),
            "canceled" => Some(JobState::Canceled),
            _ => None,
        }
    }
}

#[derive(Debug, Clone, sqlx::FromRow, Serialize)]
pub struct Job {
    pub id: Uuid,
    pub company_id: Option<Uuid>,
    pub kind: String,
    pub payload: Value,
    pub idempotency_key: Option<String>,
    pub state: String,
    pub attempts: i32,
    pub max_attempts: i32,
    pub priority: i32,
    pub available_at: DateTime<Utc>,
    pub lease_owner: Option<String>,
    pub lease_token: Option<Uuid>,
    pub lease_expires_at: Option<DateTime<Utc>>,
    pub canceled_at: Option<DateTime<Utc>>,
    pub last_error: Option<String>,
    pub result: Option<Value>,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

impl Job {
    pub fn state(&self) -> JobState {
        JobState::parse(&self.state).unwrap_or(JobState::Queued)
    }
}

#[derive(Debug, Clone)]
pub struct NewJob {
    pub company_id: Option<Uuid>,
    pub kind: String,
    pub payload: Value,
    pub idempotency_key: Option<String>,
    pub max_attempts: i32,
    pub priority: i32,
    pub available_at: Option<DateTime<Utc>>,
}

impl NewJob {
    pub fn new(company_id: Option<Uuid>, kind: impl Into<String>, payload: Value) -> Self {
        NewJob {
            company_id,
            kind: kind.into(),
            payload,
            idempotency_key: None,
            max_attempts: 5,
            priority: 0,
            available_at: None,
        }
    }

    pub fn with_idempotency_key(mut self, key: impl Into<String>) -> Self {
        self.idempotency_key = Some(key.into());
        self
    }

    pub fn with_max_attempts(mut self, n: i32) -> Self {
        self.max_attempts = n.max(1);
        self
    }
}

#[derive(Debug, Clone, Serialize)]
pub enum EnqueueOutcome {
    Enqueued(Job),
    Duplicate(Job),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum EffectOutcome {
    Pending,
    Confirmed,
    Failed,
}

impl EffectOutcome {
    pub fn as_str(self) -> &'static str {
        match self {
            EffectOutcome::Pending => "pending",
            EffectOutcome::Confirmed => "confirmed",
            EffectOutcome::Failed => "failed",
        }
    }
}

#[derive(Debug, Clone, sqlx::FromRow, Serialize)]
pub struct JobEffect {
    pub id: Uuid,
    pub job_id: Uuid,
    pub effect_key: String,
    pub request_hash: String,
    pub outcome: String,
    pub detail: Option<Value>,
    pub created_at: DateTime<Utc>,
    pub resolved_at: Option<DateTime<Utc>>,
}

#[derive(Debug, Clone, sqlx::FromRow, Serialize)]
pub struct JobSchedule {
    pub id: Uuid,
    pub company_id: Option<Uuid>,
    pub kind: String,
    pub payload: Value,
    pub interval_seconds: Option<i32>,
    pub cron: Option<String>,
    pub next_run_at: DateTime<Utc>,
    pub enabled: bool,
    pub last_job_id: Option<Uuid>,
    pub created_at: DateTime<Utc>,
}

#[derive(Debug, Clone)]
pub struct NewSchedule {
    pub company_id: Option<Uuid>,
    pub kind: String,
    pub payload: Value,
    pub interval_seconds: Option<i32>,
    pub cron: Option<String>,
}

/// Enqueue a job. With an idempotency key, a second enqueue of the same key
/// returns the existing job (Duplicate) and creates nothing.
pub async fn enqueue(pool: &PgPool, new: NewJob) -> Result<EnqueueOutcome, QueueError> {
    let id = Uuid::new_v4();
    let inserted = sqlx::query_as::<_, Job>(
        r#"
        WITH ins AS (
          INSERT INTO jobs (id, company_id, kind, payload, idempotency_key,
                            max_attempts, priority, available_at)
          VALUES ($1, $2, $3, $4, $5, $6, $7, COALESCE($8, now()))
          ON CONFLICT (company_id, idempotency_key) WHERE idempotency_key IS NOT NULL
          DO NOTHING
          RETURNING *
        ), evt AS (
          INSERT INTO job_events (job_id, from_state, to_state, detail)
          SELECT id, NULL, 'queued', '{"reason":"enqueued"}'::jsonb FROM ins
        )
        SELECT * FROM ins
        "#,
    )
    .bind(id)
    .bind(new.company_id)
    .bind(&new.kind)
    .bind(&new.payload)
    .bind(&new.idempotency_key)
    .bind(new.max_attempts)
    .bind(new.priority)
    .bind(new.available_at)
    .fetch_optional(pool)
    .await?;
    if let Some(job) = inserted {
        return Ok(EnqueueOutcome::Enqueued(job));
    }
    let existing = sqlx::query_as::<_, Job>(
        r#"SELECT * FROM jobs
           WHERE company_id IS NOT DISTINCT FROM $1
             AND kind = $2
             AND idempotency_key = $3"#,
    )
    .bind(new.company_id)
    .bind(&new.kind)
    .bind(&new.idempotency_key)
    .fetch_one(pool)
    .await?;
    Ok(EnqueueOutcome::Duplicate(existing))
}

/// Claim the next runnable job for this worker, with a fresh fencing token and
/// a lease. Empty kinds claims any kind. Attempts is incremented at claim
/// time, so a crash loop is bounded by max_attempts as well.
pub async fn claim(
    pool: &PgPool,
    worker_id: &str,
    kinds: &[String],
    lease: std::time::Duration,
) -> Result<Option<Job>, QueueError> {
    let token = Uuid::new_v4();
    let job = sqlx::query_as::<_, Job>(
        r#"
        WITH picked AS (
          SELECT id FROM jobs
          WHERE state = 'queued'
            AND available_at <= now()
            AND canceled_at IS NULL
            AND (cardinality($3::text[]) = 0 OR kind = ANY($3::text[]))
          ORDER BY priority DESC, available_at, created_at
          FOR UPDATE SKIP LOCKED
          LIMIT 1
        ), leased AS (
          UPDATE jobs j
          SET state = 'leased',
              lease_owner = $1,
              lease_token = $4,
              lease_expires_at = now() + make_interval(secs => $2::double precision),
              attempts = j.attempts + 1,
              updated_at = now()
          FROM picked
          WHERE j.id = picked.id
          RETURNING j.*
        ), evt AS (
          INSERT INTO job_events (job_id, from_state, to_state, detail)
          SELECT id, 'queued', 'leased', jsonb_build_object('worker', $1, 'attempt', attempts) FROM leased
        )
        SELECT * FROM leased
        "#,
    )
    .bind(worker_id)
    .bind(lease.as_secs_f64())
    .bind(kinds)
    .bind(token)
    .fetch_optional(pool)
    .await?;
    Ok(job)
}

/// Requeue jobs whose lease expired (worker crashed or stopped). Jobs that ran
/// out of attempts become failed; canceled jobs end as canceled. The fencing
/// token is cleared so no stale attempt can act on the job afterwards.
pub async fn reclaim_expired(pool: &PgPool) -> Result<Vec<Job>, QueueError> {
    let jobs = sqlx::query_as::<_, Job>(
        r#"
        WITH expired AS (
          UPDATE jobs
          SET state = CASE
                WHEN canceled_at IS NOT NULL THEN 'canceled'
                WHEN attempts >= max_attempts THEN 'failed'
                ELSE 'queued'
              END,
              lease_owner = NULL,
              lease_token = NULL,
              lease_expires_at = NULL,
              last_error = COALESCE(last_error, 'lease expired'),
              updated_at = now()
          WHERE state = 'leased' AND lease_expires_at < now()
          RETURNING *
        ), evt AS (
          INSERT INTO job_events (job_id, from_state, to_state, detail)
          SELECT id, 'leased', state, '{"reason":"lease_expired"}'::jsonb FROM expired
        )
        SELECT * FROM expired
        "#,
    )
    .fetch_all(pool)
    .await?;
    Ok(jobs)
}

/// Fast probe: does this attempt still hold a fresh lease on an uncanceled
/// job? Unlike heartbeat this never extends anything; workers use it to stop
/// handler work quickly on cancellation or lease loss.
pub async fn lease_held(pool: &PgPool, job: &Job) -> Result<bool, QueueError> {
    let Some(token) = job.lease_token else {
        return Ok(false);
    };
    let row: (bool,) = sqlx::query_as(
        "SELECT EXISTS (
             SELECT 1 FROM jobs
             WHERE id = $1
               AND state = 'leased'
               AND lease_owner = $2
               AND lease_token = $3
               AND lease_expires_at > now()
               AND canceled_at IS NULL
           )",
    )
    .bind(job.id)
    .bind(&job.lease_owner)
    .bind(token)
    .fetch_one(pool)
    .await?;
    Ok(row.0)
}

/// Extend the lease of a running job. Fenced: requires the matching lease
/// token and a lease that has not expired yet. Returns false when the lease is
/// stale, expired, lost to another worker or canceled.
pub async fn heartbeat(
    pool: &PgPool,
    job: &Job,
    lease: std::time::Duration,
) -> Result<bool, QueueError> {
    let Some(token) = job.lease_token else {
        return Ok(false);
    };
    let row = sqlx::query(
        r#"
        UPDATE jobs
        SET lease_expires_at = now() + make_interval(secs => $4::double precision),
            updated_at = now()
        WHERE id = $1
          AND state = 'leased'
          AND lease_owner = $2
          AND lease_token = $3
          AND lease_expires_at > now()
          AND canceled_at IS NULL
        "#,
    )
    .bind(job.id)
    .bind(&job.lease_owner)
    .bind(token)
    .bind(lease.as_secs_f64())
    .execute(pool)
    .await?;
    Ok(row.rows_affected() == 1)
}

/// Mark a leased job done. Fenced: only the current attempt with a fresh lease
/// can complete. A job canceled while running ends as canceled so the outcome
/// stays truthful.
pub async fn complete(pool: &PgPool, job: &Job, result: Value) -> Result<Job, QueueError> {
    let Some(token) = job.lease_token else {
        return Err(QueueError::Fenced);
    };
    let done = sqlx::query_as::<_, Job>(
        r#"
        WITH done AS (
          UPDATE jobs
          SET state = CASE WHEN canceled_at IS NOT NULL THEN 'canceled' ELSE 'succeeded' END,
              result = $4,
              lease_owner = NULL,
              lease_token = NULL,
              lease_expires_at = NULL,
              updated_at = now()
          WHERE id = $1
            AND state = 'leased'
            AND lease_owner = $2
            AND lease_token = $3
            AND lease_expires_at > now()
          RETURNING *
        ), evt AS (
          INSERT INTO job_events (job_id, from_state, to_state, detail)
          SELECT id, 'leased', state, '{"reason":"completed"}'::jsonb FROM done
        )
        SELECT * FROM done
        "#,
    )
    .bind(job.id)
    .bind(&job.lease_owner)
    .bind(token)
    .bind(&result)
    .fetch_optional(pool)
    .await?;
    done.ok_or(QueueError::Fenced)
}

/// Record a failed attempt with bounded retry: below max_attempts the job is
/// requeued with backoff, otherwise it becomes failed. Fenced like complete.
pub async fn fail(
    pool: &PgPool,
    job: &Job,
    error: &str,
    backoff: std::time::Duration,
) -> Result<Job, QueueError> {
    let Some(token) = job.lease_token else {
        return Err(QueueError::Fenced);
    };
    let failed = sqlx::query_as::<_, Job>(
        r#"
        WITH failed AS (
          UPDATE jobs
          SET state = CASE
                WHEN canceled_at IS NOT NULL THEN 'canceled'
                WHEN attempts >= max_attempts THEN 'failed'
                ELSE 'queued'
              END,
              available_at = now() + make_interval(secs => $4::double precision),
              lease_owner = NULL,
              lease_token = NULL,
              lease_expires_at = NULL,
              last_error = $3,
              updated_at = now()
          WHERE id = $1
            AND state = 'leased'
            AND lease_owner = $2
            AND lease_token = $5
            AND lease_expires_at > now()
          RETURNING *
        ), evt AS (
          INSERT INTO job_events (job_id, from_state, to_state, detail)
          SELECT id, 'leased', state, jsonb_build_object('reason', 'failed', 'error', $3) FROM failed
        )
        SELECT * FROM failed
        "#,
    )
    .bind(job.id)
    .bind(&job.lease_owner)
    .bind(error)
    .bind(backoff.as_secs_f64())
    .bind(token)
    .fetch_optional(pool)
    .await?;
    failed.ok_or(QueueError::Fenced)
}

/// Fail a leased job permanently, regardless of attempts left. Used for fatal
/// handler errors where retrying cannot help. Fenced like complete.
pub async fn fail_permanent(pool: &PgPool, job: &Job, error: &str) -> Result<Job, QueueError> {
    let Some(token) = job.lease_token else {
        return Err(QueueError::Fenced);
    };
    let failed = sqlx::query_as::<_, Job>(
        r#"
        WITH failed AS (
          UPDATE jobs
          SET state = 'failed',
              lease_owner = NULL,
              lease_token = NULL,
              lease_expires_at = NULL,
              last_error = $3,
              updated_at = now()
          WHERE id = $1
            AND state = 'leased'
            AND lease_owner = $2
            AND lease_token = $4
            AND lease_expires_at > now()
          RETURNING *
        ), evt AS (
          INSERT INTO job_events (job_id, from_state, to_state, detail)
          SELECT id, 'leased', 'failed', jsonb_build_object('reason', 'fatal', 'error', $3) FROM failed
        )
        SELECT * FROM failed
        "#,
    )
    .bind(job.id)
    .bind(&job.lease_owner)
    .bind(error)
    .bind(token)
    .fetch_optional(pool)
    .await?;
    failed.ok_or(QueueError::Fenced)
}

/// Cancel a job. Queued jobs cancel immediately; leased jobs are flagged and
/// the running worker observes the cancellation and stops.
pub async fn cancel(pool: &PgPool, job_id: Uuid) -> Result<Job, QueueError> {
    let job = sqlx::query_as::<_, Job>(
        r#"
        WITH canceled AS (
          UPDATE jobs
          SET canceled_at = now(),
              state = CASE WHEN state = 'queued' THEN 'canceled' ELSE state END,
              updated_at = now()
          WHERE id = $1 AND state IN ('queued', 'leased')
          RETURNING *
        ), evt AS (
          INSERT INTO job_events (job_id, from_state, to_state, detail)
          SELECT id, state, CASE WHEN state = 'canceled' THEN 'canceled' ELSE state END,
                 '{"reason":"cancel_requested"}'::jsonb
          FROM canceled
        )
        SELECT * FROM canceled
        "#,
    )
    .bind(job_id)
    .fetch_one(pool)
    .await?;
    Ok(job)
}

pub async fn is_canceled(pool: &PgPool, job_id: Uuid) -> Result<bool, QueueError> {
    let row: (bool,) = sqlx::query_as(
        r#"SELECT EXISTS (SELECT 1 FROM jobs WHERE id = $1 AND canceled_at IS NOT NULL)"#,
    )
    .bind(job_id)
    .fetch_one(pool)
    .await?;
    Ok(row.0)
}

pub async fn get_job(pool: &PgPool, job_id: Uuid) -> Result<Option<Job>, QueueError> {
    let job = sqlx::query_as::<_, Job>(r#"SELECT * FROM jobs WHERE id = $1"#)
        .bind(job_id)
        .fetch_optional(pool)
        .await?;
    Ok(job)
}

/// Record an external side effect before making the call. Calling this again
/// for the same key while pending keeps the original record.
pub async fn record_effect(
    pool: &PgPool,
    job_id: Uuid,
    effect_key: &str,
    request_hash: &str,
) -> Result<JobEffect, QueueError> {
    let effect = sqlx::query_as::<_, JobEffect>(
        r#"
        INSERT INTO job_effects (id, job_id, effect_key, request_hash)
        VALUES ($1, $2, $3, $4)
        ON CONFLICT (job_id, effect_key) DO UPDATE
          SET request_hash = EXCLUDED.request_hash
          WHERE job_effects.outcome = 'pending'
        RETURNING *
        "#,
    )
    .bind(Uuid::new_v4())
    .bind(job_id)
    .bind(effect_key)
    .bind(request_hash)
    .fetch_one(pool)
    .await?;
    Ok(effect)
}

/// Resolve a recorded effect after the external call returns.
pub async fn resolve_effect(
    pool: &PgPool,
    job_id: Uuid,
    effect_key: &str,
    outcome: EffectOutcome,
    detail: Value,
) -> Result<JobEffect, QueueError> {
    let effect = sqlx::query_as::<_, JobEffect>(
        r#"
        UPDATE job_effects
        SET outcome = $3, detail = $4, resolved_at = now()
        WHERE job_id = $1 AND effect_key = $2
        RETURNING *
        "#,
    )
    .bind(job_id)
    .bind(effect_key)
    .bind(outcome.as_str())
    .bind(&detail)
    .fetch_one(pool)
    .await?;
    Ok(effect)
}

/// Effects still pending for a job: exactly the uncertain external outcomes a
/// restarted worker must reconcile before doing anything else. They are never
/// replayed automatically.
pub async fn pending_effects(pool: &PgPool, job_id: Uuid) -> Result<Vec<JobEffect>, QueueError> {
    let effects = sqlx::query_as::<_, JobEffect>(
        r#"SELECT * FROM job_effects WHERE job_id = $1 AND outcome = 'pending' ORDER BY created_at"#,
    )
    .bind(job_id)
    .fetch_all(pool)
    .await?;
    Ok(effects)
}

/// Fire every due schedule exactly once and advance its next run time.
/// The enqueued job carries an idempotency key derived from the schedule and
/// the fire time, so even a double fire cannot duplicate work.
pub async fn fire_due_schedules(pool: &PgPool) -> Result<Vec<(JobSchedule, Job)>, QueueError> {
    let mut tx = pool.begin().await?;
    let due = sqlx::query_as::<_, JobSchedule>(
        r#"
        SELECT * FROM job_schedules
        WHERE enabled AND next_run_at <= now()
        ORDER BY next_run_at
        FOR UPDATE SKIP LOCKED
        "#,
    )
    .fetch_all(&mut *tx)
    .await?;
    let mut fired = Vec::new();
    for schedule in due {
        let next = compute_next_run(&schedule)?;
        let key = format!("schedule:{}:{}", schedule.id, schedule.next_run_at.to_rfc3339());
        let inserted = sqlx::query_as::<_, Job>(
            r#"
            WITH ins AS (
              INSERT INTO jobs (id, company_id, kind, payload, idempotency_key)
              VALUES ($1, $2, $3, $4, $5)
              ON CONFLICT (company_id, idempotency_key) WHERE idempotency_key IS NOT NULL
              DO NOTHING
              RETURNING *
            )
            SELECT * FROM ins
            "#,
        )
        .bind(Uuid::new_v4())
        .bind(schedule.company_id)
        .bind(&schedule.kind)
        .bind(&schedule.payload)
        .bind(&key)
        .fetch_optional(&mut *tx)
        .await?;
        if let Some(job) = inserted {
            sqlx::query(
                r#"
                UPDATE job_schedules
                SET next_run_at = $2, last_job_id = $3
                WHERE id = $1
                "#,
            )
            .bind(schedule.id)
            .bind(next)
            .bind(job.id)
            .execute(&mut *tx)
            .await?;
            fired.push((schedule, job));
        }
    }
    tx.commit().await?;
    Ok(fired)
}

fn compute_next_run(schedule: &JobSchedule) -> Result<DateTime<Utc>, QueueError> {
    if let Some(seconds) = schedule.interval_seconds {
        return Ok(schedule.next_run_at + chrono::Duration::seconds(seconds as i64));
    }
    if let Some(expr) = &schedule.cron {
        let cron = CronExpr::parse(expr).map_err(|e| QueueError::InvalidSchedule(e.to_string()))?;
        return cron
            .next_after(Utc::now())
            .ok_or_else(|| QueueError::InvalidSchedule("cron schedule has no future run".into()));
    }
    Err(QueueError::InvalidSchedule(
        "schedule needs interval_seconds or cron".into(),
    ))
}

/// Cancel a job inside a caller-provided tenant transaction. Row level
/// security applies: a company context can only cancel its own jobs, and a
/// foreign job id simply matches no row (not found).
pub async fn cancel_scoped(
    conn: &mut sqlx::PgConnection,
    job_id: Uuid,
) -> Result<Job, QueueError> {
    let job = sqlx::query_as::<_, Job>(
        r#"
        WITH canceled AS (
          UPDATE jobs
          SET canceled_at = now(),
              state = CASE WHEN state = 'queued' THEN 'canceled' ELSE state END,
              updated_at = now()
          WHERE id = $1 AND state IN ('queued', 'leased')
          RETURNING *
        ), evt AS (
          INSERT INTO job_events (job_id, from_state, to_state, detail)
          SELECT id, state, CASE WHEN state = 'canceled' THEN 'canceled' ELSE state END,
                 '{"reason":"cancel_requested"}'::jsonb
          FROM canceled
        )
        SELECT * FROM canceled
        "#,
    )
    .bind(job_id)
    .fetch_one(&mut *conn)
    .await?;
    Ok(job)
}

/// Read a job inside a caller-provided tenant transaction (RLS enforced).
pub async fn get_job_scoped(
    conn: &mut sqlx::PgConnection,
    job_id: Uuid,
) -> Result<Option<Job>, QueueError> {
    let job = sqlx::query_as::<_, Job>(r#"SELECT * FROM jobs WHERE id = $1"#)
        .bind(job_id)
        .fetch_optional(&mut *conn)
        .await?;
    Ok(job)
}

/// Create a schedule. The first next_run_at is now for intervals and the next
/// cron match for cron expressions.
pub async fn create_schedule(pool: &PgPool, new: NewSchedule) -> Result<JobSchedule, QueueError> {
    if new.interval_seconds.is_none() && new.cron.is_none() {
        return Err(QueueError::InvalidSchedule(
            "schedule needs interval_seconds or cron".into(),
        ));
    }
    let now = Utc::now();
    let next_run_at = match (&new.cron, new.interval_seconds) {
        (Some(expr), _) => {
            let cron = CronExpr::parse(expr).map_err(|e| QueueError::InvalidSchedule(e.to_string()))?;
            cron.next_after(now)
                .ok_or_else(|| QueueError::InvalidSchedule("cron schedule has no future run".into()))?
        }
        (None, Some(_)) => now,
        (None, None) => unreachable!(),
    };
    let schedule = sqlx::query_as::<_, JobSchedule>(
        r#"
        INSERT INTO job_schedules (id, company_id, kind, payload, interval_seconds, cron, next_run_at)
        VALUES ($1, $2, $3, $4, $5, $6, $7)
        RETURNING *
        "#,
    )
    .bind(Uuid::new_v4())
    .bind(new.company_id)
    .bind(&new.kind)
    .bind(&new.payload)
    .bind(new.interval_seconds)
    .bind(&new.cron)
    .bind(next_run_at)
    .fetch_one(pool)
    .await?;
    Ok(schedule)
}
