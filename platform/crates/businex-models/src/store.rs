//! Durable model accounting in PostgreSQL.
//!
//! Reservations are the atomic unit: token AND monetary capacity are
//! reserved BEFORE a call dispatches, then settled with observed usage and
//! cost or released. Unknown usage or cost is recorded as unknown, never as
//! zero. Totals survive process restarts because they live in the database,
//! not memory.
//!
//! Two numbers are kept apart on purpose. The conservative budget charge
//! (settled_tokens, settled_cost_micros) is what the caps see: observed
//! values when the provider reported them, the reserved estimate when it did
//! not, so an unobserved call is never free. The observed state
//! (observed_tokens, unknown_usage_calls, unknown_cost_calls, and the
//! nullable columns on model_usage) records what was actually reported, so
//! an estimate is never presented as measurement.
//!
//! Atomicity: the check, the reservation insert and the counter updates run
//! in one transaction holding a row lock on the budget line. Concurrent
//! callers queue on the lock and re-read totals, so they cannot both pass
//! the same remaining balance - for tokens and for money alike. A company
//! with no explicit policy gets an unlimited default row instead of a
//! spurious denial.
//!
//! What this does NOT provide: a cap alone is not a hard spending limit for
//! calls without a configured price. Their cost is unknowable before (and
//! sometimes after) the call, so the cap cannot bound them; companies that
//! require hard limits must set deny_when_cost_unknown - which is enforced
//! on every call, with or without a monetary cap.
//!
//! Tenancy: every transaction pins businex.company_id before touching the
//! model tables. They are FORCE ROW LEVEL SECURITY, so the application role
//! sees exactly one company's rows; without the pin the statements fail closed
//! with no rows at all. Callers pass the company id they were authorized for,
//! and settle/release refuse reservations that belong to another company.

use crate::types::{Cost, Usage};
use serde::{Deserialize, Serialize};
use sqlx::{PgPool, Postgres, Transaction};
use uuid::Uuid;

#[derive(Debug, thiserror::Error)]
pub enum StoreError {
    #[error("database error")]
    Db(#[from] sqlx::Error),
    #[error("budget denied")]
    Denied(DenyReason),
    #[error("reservation not found or already closed")]
    ReservationClosed,
    #[error("invalid estimate")]
    InvalidEstimate,
    #[error("budget accounting overflow")]
    Overflow,
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
    pub reserved_cost_micros: i64,
    pub settled_tokens: i64,
    pub settled_cost_micros: i64,
    /// Provider-reported tokens only. Zero when usage was never observed.
    pub observed_tokens: i64,
    pub unknown_usage_calls: i64,
    pub unknown_cost_calls: i64,
}

/// Pin the tenant context inside a store transaction. The setting is
/// transaction-local, so it cannot leak into the next transaction on the same
/// connection.
async fn pin_company(
    tx: &mut Transaction<'_, Postgres>,
    company_id: Uuid,
) -> Result<(), StoreError> {
    sqlx::query("SELECT set_config('businex.company_id', $1, true)")
        .bind(company_id.to_string())
        .execute(&mut **tx)
        .await?;
    Ok(())
}

/// Create or update a company budget policy. Omitted fields stay unchanged.
pub async fn set_budget(
    pool: &PgPool,
    company_id: Uuid,
    max_tokens_per_period: Option<i64>,
    max_cost_micros: Option<i64>,
    deny_when_cost_unknown: Option<bool>,
) -> Result<BudgetRow, StoreError> {
    let mut tx = pool.begin().await?;
    pin_company(&mut tx, company_id).await?;
    let row = sqlx::query_as::<
        _,
        (
            Option<i64>,
            Option<i64>,
            bool,
            i64,
            i64,
            i64,
            i64,
            i64,
            i64,
            i64,
        ),
    >(
        r#"
        INSERT INTO model_budgets (company_id, max_tokens_per_period, max_cost_micros, deny_when_cost_unknown)
        VALUES ($1, $2, $3, COALESCE($4, false))
        ON CONFLICT (company_id) DO UPDATE SET
          max_tokens_per_period = COALESCE($2, model_budgets.max_tokens_per_period),
          max_cost_micros = COALESCE($3, model_budgets.max_cost_micros),
          deny_when_cost_unknown = COALESCE($4, model_budgets.deny_when_cost_unknown),
          updated_at = now()
        RETURNING max_tokens_per_period, max_cost_micros, deny_when_cost_unknown,
                  reserved_tokens, reserved_cost_micros,
                  settled_tokens, settled_cost_micros,
                  observed_tokens, unknown_usage_calls, unknown_cost_calls
        "#,
    )
    .bind(company_id)
    .bind(max_tokens_per_period)
    .bind(max_cost_micros)
    .bind(deny_when_cost_unknown)
    .fetch_one(&mut *tx)
    .await?;
    tx.commit().await?;
    Ok(row_to_budget(row))
}

/// Atomically reserve token and monetary capacity for one call (see module
/// docs for the locking rules). cost_estimate_micros is the conservative
/// upper bound for a priced call (pricing::cost_upper_bound); None means the
/// call has no configured price and its cost is unknown. Returns the
/// reservation id to settle or release.
pub async fn reserve(
    pool: &PgPool,
    company_id: Uuid,
    estimate_tokens: i64,
    cost_estimate_micros: Option<i64>,
) -> Result<Uuid, StoreError> {
    if estimate_tokens < 0 || cost_estimate_micros.is_some_and(|c| c < 0) {
        return Err(StoreError::InvalidEstimate);
    }
    let mut tx = pool.begin().await?;
    pin_company(&mut tx, company_id).await?;
    sqlx::query(
        "INSERT INTO model_budgets (company_id) VALUES ($1)
         ON CONFLICT (company_id) DO NOTHING",
    )
    .bind(company_id)
    .execute(&mut *tx)
    .await?;
    let row: (Option<i64>, Option<i64>, bool, i64, i64, i64, i64) = sqlx::query_as(
        r#"SELECT max_tokens_per_period, max_cost_micros, deny_when_cost_unknown,
                  settled_tokens, reserved_tokens, settled_cost_micros, reserved_cost_micros
           FROM model_budgets
           WHERE company_id = $1
           FOR UPDATE"#,
    )
    .bind(company_id)
    .fetch_one(&mut *tx)
    .await?;
    let (max_tokens, max_cost, deny_unknown, settled, reserved, settled_cost, reserved_cost) = row;

    // Unknown-cost policy is enforced on every call, independent of whether
    // a monetary cap exists: a company that forbids unknown-price calls does
    // so even with no cap configured.
    if deny_unknown && cost_estimate_micros.is_none() {
        tx.rollback().await?;
        return Err(StoreError::Denied(DenyReason::UnknownCost));
    }

    // Checked arithmetic throughout: a wrapped total could pass a cap it
    // should fail.
    let committed_tokens = settled
        .checked_add(reserved)
        .and_then(|v| v.checked_add(estimate_tokens))
        .ok_or(StoreError::Overflow)?;
    if let Some(max) = max_tokens {
        if committed_tokens > max {
            tx.rollback().await?;
            return Err(StoreError::Denied(DenyReason::Tokens {
                committed: committed_tokens,
                max,
            }));
        }
    }

    let committed_cost = settled_cost
        .checked_add(reserved_cost)
        .and_then(|v| v.checked_add(cost_estimate_micros.unwrap_or(0)))
        .ok_or(StoreError::Overflow)?;
    if let Some(max) = max_cost {
        // A priced call must fit the cap alongside everything already
        // settled and still reserved. An unpriced call cannot prove it fits,
        // so it needs strict headroom to proceed at all.
        let violates_cap = match cost_estimate_micros {
            Some(_) => committed_cost > max,
            None => committed_cost >= max,
        };
        if violates_cap {
            tx.rollback().await?;
            return Err(StoreError::Denied(DenyReason::Cost {
                committed: committed_cost,
                max,
            }));
        }
    }

    let reservation_id = Uuid::new_v4();
    sqlx::query(
        "INSERT INTO model_reservations (id, company_id, tokens, cost_estimate_micros)
         VALUES ($1, $2, $3, $4)",
    )
    .bind(reservation_id)
    .bind(company_id)
    .bind(estimate_tokens)
    .bind(cost_estimate_micros)
    .execute(&mut *tx)
    .await?;
    sqlx::query(
        "UPDATE model_budgets SET
           reserved_tokens = reserved_tokens + $2,
           reserved_cost_micros = reserved_cost_micros + $3,
           updated_at = now()
         WHERE company_id = $1",
    )
    .bind(company_id)
    .bind(estimate_tokens)
    .bind(cost_estimate_micros.unwrap_or(0))
    .execute(&mut *tx)
    .await?;
    tx.commit().await?;
    Ok(reservation_id)
}

/// Settle a reservation with observed usage and cost. Unknown usage charges
/// the reserved token estimate and unknown cost charges the reserved cost
/// estimate, so no call is free; the observed columns and unknown counters
/// keep the unobserved state visible. The provider and model are recorded so
/// usage rows attribute spend to the call that produced it.
///
/// The company scoping is deliberate: a reservation belonging to another
/// company cannot be settled here even if its id is known.
#[allow(clippy::too_many_arguments)]
pub async fn settle(
    pool: &PgPool,
    company_id: Uuid,
    reservation_id: Uuid,
    provider: &str,
    model: &str,
    usage: Option<Usage>,
    cost: Cost,
) -> Result<(), StoreError> {
    settle_inner(
        pool, company_id, reservation_id, provider, model, usage, cost, "observed",
    )
    .await
}

/// Settle a reservation whose call was dispatched but whose outcome is
/// unknown (timeout or transport failure after the request left). Both the
/// token and the cost estimate stay charged, never refunded as unspent: the
/// provider may have processed the call. The usage row is marked "ambiguous"
/// so the spend can be reconciled once the provider reports actual numbers.
pub async fn settle_ambiguous(
    pool: &PgPool,
    company_id: Uuid,
    reservation_id: Uuid,
    provider: &str,
    model: &str,
) -> Result<(), StoreError> {
    settle_inner(
        pool, company_id, reservation_id, provider, model, None, Cost::Unknown, "ambiguous",
    )
    .await
}

#[allow(clippy::too_many_arguments)]
async fn settle_inner(
    pool: &PgPool,
    company_id: Uuid,
    reservation_id: Uuid,
    provider: &str,
    model: &str,
    usage: Option<Usage>,
    cost: Cost,
    outcome: &str,
) -> Result<(), StoreError> {
    let mut tx = pool.begin().await?;
    pin_company(&mut tx, company_id).await?;
    let row: Option<(i64, Option<i64>)> = sqlx::query_as(
        r#"UPDATE model_reservations
           SET state = 'settled', settled_at = now()
           WHERE id = $1 AND company_id = $2 AND state = 'open'
           RETURNING tokens, cost_estimate_micros"#,
    )
    .bind(reservation_id)
    .bind(company_id)
    .fetch_optional(&mut *tx)
    .await?;
    let (held_tokens, held_cost) = row.ok_or(StoreError::ReservationClosed)?;

    // Observed when the provider reported it; the reserved estimate
    // otherwise. The observed columns never absorb an estimate.
    let observed_total = usage.and_then(|u| u.total()).map(|t| t as i64);
    let tokens_charged = observed_total.unwrap_or(held_tokens);
    let (observed_cost, cost_known) = match cost {
        Cost::Known { micros } if micros >= 0 => (Some(micros), true),
        _ => (None, false),
    };
    let cost_charged = observed_cost.unwrap_or_else(|| held_cost.unwrap_or(0));

    sqlx::query(
        r#"UPDATE model_budgets SET
             reserved_tokens = GREATEST(reserved_tokens - $2, 0),
             reserved_cost_micros = GREATEST(reserved_cost_micros - $3, 0),
             settled_tokens = settled_tokens + $4,
             settled_cost_micros = settled_cost_micros + $5,
             observed_tokens = observed_tokens + $6,
             unknown_usage_calls = unknown_usage_calls + $7,
             unknown_cost_calls = unknown_cost_calls + $8,
             updated_at = now()
           WHERE company_id = $1"#,
    )
    .bind(company_id)
    .bind(held_tokens)
    .bind(held_cost.unwrap_or(0))
    .bind(tokens_charged)
    .bind(cost_charged)
    .bind(observed_total.unwrap_or(0))
    .bind(i64::from(observed_total.is_none()))
    .bind(i64::from(!cost_known))
    .execute(&mut *tx)
    .await?;
    sqlx::query(
        r#"INSERT INTO model_usage
           (id, company_id, provider, model, input_tokens, output_tokens, cost_micros, cost_known, reservation_id, outcome)
           SELECT $1, $2, $3, $4, $5, $6, $7, $8, $9, $10"#,
    )
    .bind(Uuid::new_v4())
    .bind(company_id)
    .bind(provider)
    .bind(model)
    .bind(usage.and_then(|u| u.input_tokens).map(|t| t as i64))
    .bind(usage.and_then(|u| u.output_tokens).map(|t| t as i64))
    .bind(observed_cost)
    .bind(cost_known)
    .bind(reservation_id)
    .bind(outcome)
    .execute(&mut *tx)
    .await?;
    tx.commit().await?;
    Ok(())
}

/// Release a reservation without accounting. Only valid when nothing was
/// dispatched: after dispatch the outcome is unknown and must be settled
/// through settle_ambiguous instead.
/// Company scoped like settle.
pub async fn release(
    pool: &PgPool,
    company_id: Uuid,
    reservation_id: Uuid,
) -> Result<(), StoreError> {
    let mut tx = pool.begin().await?;
    pin_company(&mut tx, company_id).await?;
    let row: Option<(i64, Option<i64>)> = sqlx::query_as(
        r#"UPDATE model_reservations
           SET state = 'released', settled_at = now()
           WHERE id = $1 AND company_id = $2 AND state = 'open'
           RETURNING tokens, cost_estimate_micros"#,
    )
    .bind(reservation_id)
    .bind(company_id)
    .fetch_optional(&mut *tx)
    .await?;
    let (tokens, held_cost) = row.ok_or(StoreError::ReservationClosed)?;
    sqlx::query(
        "UPDATE model_budgets SET
           reserved_tokens = GREATEST(reserved_tokens - $2, 0),
           reserved_cost_micros = GREATEST(reserved_cost_micros - $3, 0),
           updated_at = now()
         WHERE company_id = $1",
    )
    .bind(company_id)
    .bind(tokens)
    .bind(held_cost.unwrap_or(0))
    .execute(&mut *tx)
    .await?;
    tx.commit().await?;
    Ok(())
}

type BudgetTuple = (Option<i64>, Option<i64>, bool, i64, i64, i64, i64, i64, i64, i64);

fn row_to_budget(r: BudgetTuple) -> BudgetRow {
    BudgetRow {
        max_tokens_per_period: r.0,
        max_cost_micros: r.1,
        deny_when_cost_unknown: r.2,
        reserved_tokens: r.3,
        reserved_cost_micros: r.4,
        settled_tokens: r.5,
        settled_cost_micros: r.6,
        observed_tokens: r.7,
        unknown_usage_calls: r.8,
        unknown_cost_calls: r.9,
    }
}

pub async fn get_budget(pool: &PgPool, company_id: Uuid) -> Result<Option<BudgetRow>, StoreError> {
    let mut tx = pool.begin().await?;
    pin_company(&mut tx, company_id).await?;
    let row = sqlx::query_as::<_, BudgetTuple>(
        r#"SELECT max_tokens_per_period, max_cost_micros, deny_when_cost_unknown,
                  reserved_tokens, reserved_cost_micros,
                  settled_tokens, settled_cost_micros,
                  observed_tokens, unknown_usage_calls, unknown_cost_calls
           FROM model_budgets WHERE company_id = $1"#,
    )
    .bind(company_id)
    .fetch_optional(&mut *tx)
    .await?;
    tx.commit().await?;
    Ok(row.map(row_to_budget))
}
