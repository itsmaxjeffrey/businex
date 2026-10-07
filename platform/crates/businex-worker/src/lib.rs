//! Durable job workers.
//!
//! Workers claim jobs from the PostgreSQL queue with a lease and a fencing
//! token, and heartbeat while running. If a worker dies, the lease expires and
//! another worker retries the job: execution survives process restarts and
//! client disconnects. A stale attempt (expired lease, or an attempt replaced
//! by a newer claim even with the same worker id) is fenced out and cannot
//! complete, fail or refresh the job.

use async_trait::async_trait;
use businex_events::Relay;
use businex_queue::{self as queue, Job, JobEffect, QueueError};
use serde_json::Value;
use std::collections::HashMap;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::time::Duration;

#[derive(Debug, thiserror::Error)]
pub enum HandlerError {
    #[error("retryable: {0}")]
    Retryable(String),
    #[error("fatal: {0}")]
    Fatal(String),
}

/// What a handler produced: a JSON result stored on the job.
pub type HandlerResult = Result<Value, HandlerError>;

#[async_trait]
pub trait JobHandler: Send + Sync {
    /// Execute one job attempt. The context exposes cancellation, lease
    /// heartbeat status and external-effect records.
    async fn handle(&self, ctx: &JobContext) -> HandlerResult;

    /// Called instead of handle() when the job has unresolved external
    /// effects (the previous attempt may have performed them). The default
    /// refuses to run: a blind retry could double a webhook or payment.
    /// Handlers that can check the external system implement this, resolve
    /// the effects, and then work proceeds. Pending effects are never
    /// replayed automatically.
    async fn reconcile(&self, ctx: &JobContext, effects: &[JobEffect]) -> Result<(), HandlerError> {
        let _ = (ctx, effects);
        Err(HandlerError::Fatal(
            "uncertain external outcome requires reconciliation".into(),
        ))
    }
}

pub struct JobContext {
    pub pool: sqlx::PgPool,
    pub job: Job,
    pub worker_id: String,
    pub relay: Relay,
    lost: Arc<AtomicBool>,
}

impl JobContext {
    /// True once the job is canceled: long-running handlers should check this
    /// between steps and stop.
    pub async fn is_canceled(&self) -> Result<bool, QueueError> {
        queue::is_canceled(&self.pool, self.job.id).await
    }

    /// True when the lease was lost to another worker or the job canceled.
    pub fn lost(&self) -> bool {
        self.lost.load(Ordering::Relaxed)
    }

    /// Record an external side effect before calling out; resolve it after.
    pub async fn record_effect(&self, key: &str, request_hash: &str) -> Result<JobEffect, QueueError> {
        queue::record_effect(&self.pool, self.job.id, key, request_hash).await
    }

    pub async fn resolve_effect(
        &self,
        key: &str,
        outcome: queue::EffectOutcome,
        detail: Value,
    ) -> Result<JobEffect, QueueError> {
        queue::resolve_effect(&self.pool, self.job.id, key, outcome, detail).await
    }

    /// Extend the lease explicitly for long steps. Fenced like all lease
    /// operations: stale attempts get false.
    pub async fn heartbeat(&self) -> Result<bool, QueueError> {
        queue::heartbeat(&self.pool, &self.job, DEFAULT_LEASE).await
    }
}

pub const DEFAULT_LEASE: Duration = Duration::from_secs(60);

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct JobReport {
    pub job_id: uuid::Uuid,
    pub outcome: String,
}

pub struct Worker {
    pool: sqlx::PgPool,
    worker_id: String,
    relay: Relay,
    handlers: HashMap<String, Arc<dyn JobHandler>>,
    lease: Duration,
    poll_interval: Duration,
}

impl Worker {
    pub fn new(pool: sqlx::PgPool, relay: Relay) -> Worker {
        Worker {
            pool,
            worker_id: format!("worker-{}", uuid::Uuid::new_v4()),
            relay,
            handlers: HashMap::new(),
            lease: DEFAULT_LEASE,
            poll_interval: Duration::from_millis(250),
        }
    }

    pub fn with_worker_id(mut self, id: impl Into<String>) -> Self {
        self.worker_id = id.into();
        self
    }

    pub fn with_lease(mut self, lease: Duration) -> Self {
        self.lease = lease;
        self
    }

    pub fn with_poll_interval(mut self, interval: Duration) -> Self {
        self.poll_interval = interval;
        self
    }

    pub fn register(&mut self, kind: impl Into<String>, handler: Arc<dyn JobHandler>) {
        self.handlers.insert(kind.into(), handler);
    }

    pub fn worker_id(&self) -> &str {
        &self.worker_id
    }

    /// One maintenance + dispatch cycle: reclaim expired leases, fire due
    /// schedules, then claim and execute at most one job.
    pub async fn run_once(&self) -> Result<Option<JobReport>, QueueError> {
        queue::reclaim_expired(&self.pool).await?;
        queue::fire_due_schedules(&self.pool).await?;

        if self.handlers.is_empty() {
            return Ok(None);
        }
        let kinds: Vec<String> = self.handlers.keys().cloned().collect();
        let Some(job) = queue::claim(&self.pool, &self.worker_id, &kinds, self.lease).await? else {
            return Ok(None);
        };
        let handler = self
            .handlers
            .get(&job.kind)
            .expect("claimed job kind must have a handler")
            .clone();

        let lost = Arc::new(AtomicBool::new(false));
        let ctx = JobContext {
            pool: self.pool.clone(),
            job: job.clone(),
            worker_id: self.worker_id.clone(),
            relay: self.relay.clone(),
            lost: lost.clone(),
        };

        // Heartbeat task keeps the lease alive while the handler runs. If the
        // lease is fenced out (expired or superseded) the flag is raised and
        // the handler can stop early through ctx.lost().
        let hb_pool = self.pool.clone();
        let hb_job = job.clone();
        let hb_lost = lost.clone();
        let hb_lease = self.lease;
        let heartbeat = tokio::spawn(async move {
            let mut interval = tokio::time::interval(hb_lease / 3);
            interval.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Delay);
            loop {
                interval.tick().await;
                match queue::heartbeat(&hb_pool, &hb_job, hb_lease).await {
                    Ok(true) => {}
                    Ok(false) => {
                        hb_lost.store(true, Ordering::Relaxed);
                        break;
                    }
                    Err(_) => {}
                }
            }
        });

        let run_result = if queue::is_canceled(&self.pool, job.id).await? {
            Err(HandlerError::Fatal("canceled".into()))
        } else {
            let effects = queue::pending_effects(&self.pool, job.id).await?;
            if effects.is_empty() {
                handler.handle(&ctx).await
            } else {
                match handler.reconcile(&ctx, &effects).await {
                    Ok(()) => handler.handle(&ctx).await,
                    Err(err) => Err(err),
                }
            }
        };
        heartbeat.abort();

        let report = match run_result {
            Ok(value) => match queue::complete(&self.pool, &ctx.job, value).await {
                Ok(done) => JobReport { job_id: job.id, outcome: done.state },
                Err(QueueError::Fenced) => JobReport { job_id: job.id, outcome: "fenced".into() },
                Err(err) => return Err(err),
            },
            Err(HandlerError::Fatal(reason)) if reason == "canceled" => {
                match queue::complete(&self.pool, &ctx.job, Value::Null).await {
                    Ok(done) => JobReport { job_id: job.id, outcome: done.state },
                    Err(QueueError::Fenced) => JobReport { job_id: job.id, outcome: "fenced".into() },
                    Err(err) => return Err(err),
                }
            }
            Err(HandlerError::Fatal(reason)) => {
                match queue::fail_permanent(&self.pool, &ctx.job, &reason).await {
                    Ok(done) => JobReport { job_id: job.id, outcome: done.state },
                    Err(QueueError::Fenced) => JobReport { job_id: job.id, outcome: "fenced".into() },
                    Err(err) => return Err(err),
                }
            }
            Err(HandlerError::Retryable(reason)) => {
                let backoff = Duration::from_secs_f64((2f64.powi(job.attempts.min(6))).min(60.0));
                match queue::fail(&self.pool, &ctx.job, &reason, backoff).await {
                    Ok(done) => JobReport { job_id: job.id, outcome: done.state },
                    Err(QueueError::Fenced) => JobReport { job_id: job.id, outcome: "fenced".into() },
                    Err(err) => return Err(err),
                }
            }
        };
        Ok(Some(report))
    }

    /// Run until shutdown flips to true. Jobs in flight finish first because
    /// claiming stops; leases keep them safe either way.
    pub async fn run(&self, mut shutdown: tokio::sync::watch::Receiver<bool>) {
        loop {
            if *shutdown.borrow() {
                break;
            }
            match self.run_once().await {
                Ok(Some(report)) => {
                    tracing::info!(job_id = %report.job_id, outcome = %report.outcome, "job finished");
                }
                Ok(None) => {}
                Err(_err) => {
                    tracing::warn!("worker cycle failed");
                }
            }
            tokio::select! {
                _ = shutdown.changed() => {}
                _ = tokio::time::sleep(self.poll_interval) => {}
            }
        }
        tracing::info!(worker = %self.worker_id, "worker stopped");
    }
}
