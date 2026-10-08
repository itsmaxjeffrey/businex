//! Durable model accounting in PostgreSQL.
//!
//! Reservations are the atomic unit: capacity is reserved BEFORE a call
//! dispatches, then settled with observed usage and cost or released. Unknown
//! cost is recorded as unknown, never as zero. Totals survive process restarts
//! because they live in the database, not memory.
//!
//! Atomicity: the check, the reservation insert and the counter update run in
//! one transaction holding a row lock on the budget line. Concurrent callers
//! queue on the lock and re-read totals, so they cannot both pass the same
//! remaining balance. A company with no explicit policy gets an unlimited
//! default row instead of a spurious denial.

use crate::types::{Cost, Usage};
use serde::{Deserialize, Serialize};
use sqlx::PgPool;
use uuid::Uuid;

#[derive(Debug, thiserror::Error)]
pub enum StoreError {
    #[error("database error")]
    Db(#[from] sqlx::Error),
    #[error("budget denied")]
    Denied(DenyReason),
    #[error("reservation not found or already closed")]
    ReservationClosed,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "state", rename_all = "lowercase")]
pub enum DenyReason {
    Tokens { committed: i64, max: i64 },
    Cost { committed: i64, max: i64 },
    UnknownCost,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct BudgetRow {
    pub max_tokens_per_period: Option<i64>,
    pub max_cost_micros: Option<i64>,
    pub deny_when_cost_unknown: bool,
    pub reserved_tokens: i64,
    pub settled_tokens: i64,
    pub settled_cost_micros: i64,
    pub unknown_cost_calls: i64,
}

/// Create or update a company budget policy. Omitted fields stay unchanged.
pub async fn set_budget(
    pool: &PgPool,
    company_id: Uuid,
    max_tokens_per_period: Option<i64>,
    max_cost_micros: Option<i64>,
    deny_when_cost_unknown: Option<bool>,
) -> Result<BudgetRow, StoreError> {
    let row = sqlx::query_as::<_, (Option<i64>, Option<i64>, bool, i64, i64, i64, i64)>(
        r#"
        INSERT INTO model_budgets (company_id, max_tokens_per_period, max_cost_micros, deny_when_cost_unknown)
        VALUES ($1, $2, $3, COALESCE($4, false))
        ON CONFLICT (company_id) DO UPDATE SET
          max_tokens_per_period = COALESCE($2, model_budgets.max_tokens_per_period),
          max_cost_micros = COALESCE($3, model_budgets.max_cost_micros),
          deny_when_cost_unknown = COALESCE($4, model_budgets.deny_when_cost_unknown),
          updated_at = now()
        RETURNING max_tokens_per_period, max_cost_micros, deny_when_cost_unknown,
                  reserved_tokens, settled_tokens, settled_cost_micros, unknown_cost_calls
        "#,
    )
    .bind(company_id)
    .bind(max_tokens_per_period)
    .bind(max_cost_micros)
    .bind(deny_when_cost_unknown)
    .fetch_one(pool)
    .await?;
    Ok(BudgetRow {
        max_tokens_per_period: row.0,
        max_cost_micros: row.1,
        deny_when_cost_unknown: row.2,
        reserved_tokens: row.3,
        settled_tokens: row.4,
        settled_cost_micros: row.5,
        unknown_cost_calls: row.6,
    })
}

/// Atomically reserve capacity for one call (see module docs for the locking
/// rules). Returns the reservation id to settle or release.
pub async fn reserve(
    pool: &PgPool,
    company_id: Uuid,
    estimate_tokens: i64,
) -> Result<Uuid, StoreError> {
    let mut tx = pool.begin().await?;
    sqlx::query(
        "INSERT INTO model_budgets (company_id) VALUES ($1)
         ON CONFLICT (company_id) DO NOTHING",
    )
    .bind(company_id)
    .execute(&mut *tx)
    .await?;
    let row: (Option<i64>, Option<i64>, bool, i64, i64, i64) = sqlx::query_as(
        r#"SELECT max_tokens_per_period, max_cost_micros, deny_when_cost_unknown,
                  settled_tokens, reserved_tokens, settled_cost_micros
           FROM model_budgets
           WHERE company_id = $1
           FOR UPDATE"#,
    )
    .bind(company_id)
    .fetch_optional(&mut *tx)
    .await?
    .ok_or_else(|| StoreError::Denied(DenyReason::UnknownCost))?;
    let (max_tokens, max_cost, deny_unknown, settled, reserved, settled_cost) = row;

    if let Some(max) = max_tokens {
        let committed = settled + reserved + estimate_tokens;
        if committed > max {
            tx.rollback().await?;
            return Err(StoreError::Denied(DenyReason::Tokens { committed, max }));
        }
    }
    if let Some(max) = max_cost {
        if settled_cost >= max {
            tx.rollback().await?;
            return Err(StoreError::Denied(DenyReason::Cost {
                committed: settled_cost,
                max,
            }));
        }
        if deny_unknown {
            // The cost of a call is unknown before dispatch; policy forbids it.
            tx.rollback().await?;
            return Err(StoreError::Denied(DenyReason::UnknownCost));
        }
    }

    let reservation_id = Uuid::new_v4();
    sqlx::query("INSERT INTO model_reservations (id, company_id, tokens) VALUES ($1, $2, $3)")
        .bind(reservation_id)
        .bind(company_id)
        .bind(estimate_tokens)
        .execute(&mut *tx)
        .await?;
    sqlx::query(
        "UPDATE model_budgets SET reserved_tokens = reserved_tokens + $2, updated_at = now()
         WHERE company_id = $1",
    )
    .bind(company_id)
    .bind(estimate_tokens)
    .execute(&mut *tx)
    .await?;
    tx.commit().await?;
    Ok(reservation_id)
}

/// Settle a reservation with observed usage and cost. Unknown usage charges
/// the reservation estimate; unknown cost increments the unknown counter and
/// never the cost total.
pub async fn settle(
    pool: &PgPool,
    reservation_id: Uuid,
    usage: Option<Usage>,
    cost: Cost,
) -> Result<(), StoreError> {
    let mut tx = pool.begin().await?;
    let row: (Uuid, i64) = sqlx::query_as(
        r#"UPDATE model_reservations
           SET state = 'settled', settled_at = now()
           WHERE id = $1 AND state = 'open'
           RETURNING company_id, tokens"#,
    )
    .bind(reservation_id)
    .fetch_optional(&mut *tx)
    .await?
    .ok_or(StoreError::ReservationClosed)?;
    let (company_id, reserved) = row;
    let observed = usage.and_then(|u| u.total()).map(|t| t as i64);
    let tokens = observed.unwrap_or(reserved);
    let (cost_micros, cost_known) = match cost {
        Cost::Known { micros } if micros >= 0 => (Some(micros), true),
        _ => (None, false),
    };
    sqlx::query(
        r#"UPDATE model_budgets SET
             reserved_tokens = GREATEST(reserved_tokens - $2, 0),
             settled_tokens = settled_tokens + $3,
             settled_cost_micros = settled_cost_micros + COALESCE($4, 0),
             unknown_cost_calls = unknown_cost_calls + CASE WHEN $5 THEN 0 ELSE 1 END,
             updated_at = now()
           WHERE company_id = $1"#,
    )
    .bind(company_id)
    .bind(reserved)
    .bind(tokens)
    .bind(cost_micros)
    .bind(cost_known)
    .execute(&mut *tx)
    .await?;
    sqlx::query(
        r#"INSERT INTO model_usage
           (id, company_id, provider, model, input_tokens, output_tokens, cost_micros, cost_known, reservation_id)
           SELECT $1, $2, $3, $4, $5, $6, $7, $8, $9"#,
    )
    .bind(Uuid::new_v4())
    .bind(company_id)
    .bind("unknown")
    .bind("unknown")
    .bind(usage.and_then(|u| u.input_tokens).map(|t| t as i64))
    .bind(usage.and_then(|u| u.output_tokens).map(|t| t as i64))
    .bind(cost_micros)
    .bind(cost_known)
    .bind(reservation_id)
    .execute(&mut *tx)
    .await?;
    tx.commit().await?;
    Ok(())
}

/// Release a reservation without accounting (call failed before usage).
pub async fn release(pool: &PgPool, reservation_id: Uuid) -> Result<(), StoreError> {
    let mut tx = pool.begin().await?;
    let row: (Uuid, i64) = sqlx::query_as(
        r#"UPDATE model_reservations
           SET state = 'released', settled_at = now()
           WHERE id = $1 AND state = 'open'
           RETURNING company_id, tokens"#,
    )
    .bind(reservation_id)
    .fetch_optional(&mut *tx)
    .await?
    .ok_or(StoreError::ReservationClosed)?;
    sqlx::query(
        "UPDATE model_budgets SET reserved_tokens = GREATEST(reserved_tokens - $2, 0), updated_at = now()
         WHERE company_id = $1",
    )
    .bind(row.0)
    .bind(row.1)
    .execute(&mut *tx)
    .await?;
    tx.commit().await?;
    Ok(())
}

pub async fn get_budget(pool: &PgPool, company_id: Uuid) -> Result<Option<BudgetRow>, StoreError> {
    let row = sqlx::query_as::<_, (Option<i64>, Option<i64>, bool, i64, i64, i64, i64)>(
        r#"SELECT max_tokens_per_period, max_cost_micros, deny_when_cost_unknown,
                  reserved_tokens, settled_tokens, settled_cost_micros, unknown_cost_calls
           FROM model_budgets WHERE company_id = $1"#,
    )
    .bind(company_id)
    .fetch_optional(pool)
    .await?;
    Ok(row.map(|r| BudgetRow {
        max_tokens_per_period: r.0,
        max_cost_micros: r.1,
        deny_when_cost_unknown: r.2,
        reserved_tokens: r.3,
        settled_tokens: r.4,
        settled_cost_micros: r.5,
        unknown_cost_calls: r.6,
    }))
}
