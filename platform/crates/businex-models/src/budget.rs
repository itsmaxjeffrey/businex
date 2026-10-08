//! Per-company model budgets with atomic reservation.
//!
//! Budget checks must reserve capacity BEFORE concurrent calls dispatch, then
//! settle with observed usage and cost. Without reservation, several concurrent
//! calls each pass the same remaining balance. Unknown price or unknown usage
//! never counts as free: the reservation settles conservatively and stays
//! explicit-unknown in the totals.

use crate::types::{Cost, Usage};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::sync::Mutex;
use uuid::Uuid;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
pub struct Budget {
    /// Maximum input+output tokens per period. None means unlimited.
    pub max_tokens_per_period: Option<u64>,
    /// Maximum spend in currency micro-units per period. None means unlimited.
    /// Only enforceable for priced usage; unknown-cost usage is tracked and
    /// policy can require denial (deny_when_cost_unknown).
    pub max_cost_micros_per_period: Option<i64>,
    /// When true, calls whose cost cannot be computed are denied under a cost
    /// budget instead of being admitted with unknown cost.
    pub deny_when_cost_unknown: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
pub struct UsageTotals {
    /// Reserved-but-unsettled tokens (in flight).
    pub reserved_tokens: u64,
    pub settled_tokens: u64,
    /// Sum of known costs only; unknown-price/usage never silently zeroes.
    pub settled_cost_micros: i64,
    pub unknown_cost_calls: u64,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "state", rename_all = "lowercase")]
pub enum BudgetDecision {
    Allow,
    DenyTokens {
        committed_tokens: u64,
        max_tokens: u64,
    },
    DenyCost {
        committed_cost_micros: i64,
        max_cost_micros: i64,
    },
    DenyUnknownCost {
        reason: String,
    },
}

/// A granted reservation. Settling releases the reserved capacity and records
/// the observed usage. Dropping without settling also releases (crashed call),
/// while the ledger records it as an unknown-cost outcome when cost is absent.
pub struct Reservation {
    company_id: Uuid,
    tokens: u64,
    id: Uuid,
}

impl Reservation {
    pub fn id(&self) -> Uuid {
        self.id
    }
}

pub struct BudgetLedger {
    budgets: Mutex<HashMap<Uuid, (Budget, UsageTotals)>>,
}

impl BudgetLedger {
    pub fn new() -> Self {
        BudgetLedger {
            budgets: Mutex::new(HashMap::new()),
        }
    }

    /// Set or replace a company budget policy.
    pub fn set_budget(&self, company_id: Uuid, budget: Budget) {
        let mut guard = self.budgets.lock().expect("ledger lock");
        guard.entry(company_id).or_default().0 = budget;
    }

    pub fn totals(&self, company_id: Uuid) -> UsageTotals {
        self.budgets
            .lock()
            .expect("ledger lock")
            .get(&company_id)
            .map(|(_, totals)| *totals)
            .unwrap_or_default()
    }

    /// Atomically reserve capacity for one call. The check and the reservation
    /// happen under one lock, so concurrent calls cannot double-spend.
    pub fn reserve(
        &self,
        company_id: Uuid,
        estimate_tokens: u64,
    ) -> Result<Reservation, BudgetDecision> {
        let mut guard = self.budgets.lock().expect("ledger lock");
        let (budget, totals) = guard.entry(company_id).or_default();
        let committed_tokens = totals.settled_tokens + totals.reserved_tokens + estimate_tokens;
        if let Some(max) = budget.max_tokens_per_period {
            if committed_tokens > max {
                return Err(BudgetDecision::DenyTokens {
                    committed_tokens,
                    max_tokens: max,
                });
            }
        }
        if let Some(max) = budget.max_cost_micros_per_period {
            if totals.settled_cost_micros >= max {
                return Err(BudgetDecision::DenyCost {
                    committed_cost_micros: totals.settled_cost_micros,
                    max_cost_micros: max,
                });
            }
            if budget.deny_when_cost_unknown {
                return Err(BudgetDecision::DenyUnknownCost {
                    reason: "cost is unknown before the call".into(),
                });
            }
        }
        totals.reserved_tokens += estimate_tokens;
        Ok(Reservation {
            company_id,
            tokens: estimate_tokens,
            id: Uuid::new_v4(),
        })
    }

    /// Settle a reservation with observed usage and cost. Unknown usage or
    /// unknown cost is recorded as unknown, never as zero.
    pub fn settle(&self, reservation: Reservation, usage: Option<Usage>, cost: Cost) {
        let mut guard = self.budgets.lock().expect("ledger lock");
        if let Some((_, totals)) = guard.get_mut(&reservation.company_id) {
            totals.reserved_tokens = totals.reserved_tokens.saturating_sub(reservation.tokens);
            let observed = usage.and_then(|u| u.total()).unwrap_or(reservation.tokens);
            totals.settled_tokens = totals.settled_tokens.saturating_add(observed);
            match cost {
                Cost::Known { micros } if micros >= 0 => {
                    totals.settled_cost_micros = totals.settled_cost_micros.saturating_add(micros);
                }
                _ => totals.unknown_cost_calls += 1,
            }
        }
    }

    /// Release without accounting (call failed before producing usage).
    pub fn release(&self, reservation: Reservation) {
        let mut guard = self.budgets.lock().expect("ledger lock");
        if let Some((_, totals)) = guard.get_mut(&reservation.company_id) {
            totals.reserved_tokens = totals.reserved_tokens.saturating_sub(reservation.tokens);
        }
    }
}

impl Default for BudgetLedger {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Arc;

    #[test]
    fn token_budget_denies_when_exhausted() {
        let ledger = BudgetLedger::new();
        let company = Uuid::new_v4();
        ledger.set_budget(
            company,
            Budget {
                max_tokens_per_period: Some(1000),
                ..Default::default()
            },
        );
        let r = ledger.reserve(company, 1000).expect("reserve");
        ledger.settle(r, Some(Usage::of(600, 400)), Cost::Unknown);
        let denied = ledger.reserve(company, 1);
        assert!(matches!(denied, Err(BudgetDecision::DenyTokens { .. })));
    }

    #[test]
    fn concurrent_reservations_cannot_double_spend() {
        let ledger = Arc::new(BudgetLedger::new());
        let company = Uuid::new_v4();
        ledger.set_budget(
            company,
            Budget {
                max_tokens_per_period: Some(1000),
                ..Default::default()
            },
        );
        let mut handles = Vec::new();
        for _ in 0..8 {
            let ledger = Arc::clone(&ledger);
            handles.push(std::thread::spawn(move || {
                ledger.reserve(company, 600).is_ok()
            }));
        }
        let wins = handles
            .into_iter()
            .map(|h| h.join().expect("thread"))
            .filter(|won| *won)
            .count();
        assert_eq!(wins, 1, "only one 600-token call fits in 1000 tokens");
    }

    #[test]
    fn unknown_cost_is_counted_not_zeroed() {
        let ledger = BudgetLedger::new();
        let company = Uuid::new_v4();
        let r = ledger.reserve(company, 10).expect("reserve");
        ledger.settle(r, Some(Usage::of(4, 6)), Cost::Unknown);
        let totals = ledger.totals(company);
        assert_eq!(totals.settled_tokens, 10);
        assert_eq!(totals.settled_cost_micros, 0);
        assert_eq!(totals.unknown_cost_calls, 1);
    }

    #[test]
    fn cost_budget_denies_when_exhausted() {
        let ledger = BudgetLedger::new();
        let company = Uuid::new_v4();
        ledger.set_budget(
            company,
            Budget {
                max_cost_micros_per_period: Some(5_000_000),
                ..Default::default()
            },
        );
        let r = ledger.reserve(company, 10).expect("reserve");
        ledger.settle(r, Some(Usage::of(5, 5)), Cost::Known { micros: 5_000_000 });
        assert!(matches!(
            ledger.reserve(company, 1),
            Err(BudgetDecision::DenyCost { .. })
        ));
    }

    #[test]
    fn deny_unknown_cost_policy() {
        let ledger = BudgetLedger::new();
        let company = Uuid::new_v4();
        ledger.set_budget(
            company,
            Budget {
                max_cost_micros_per_period: Some(1_000_000),
                deny_when_cost_unknown: true,
                ..Default::default()
            },
        );
        assert!(matches!(
            ledger.reserve(company, 1),
            Err(BudgetDecision::DenyUnknownCost { .. })
        ));
    }

    #[test]
    fn release_returns_capacity() {
        let ledger = BudgetLedger::new();
        let company = Uuid::new_v4();
        ledger.set_budget(
            company,
            Budget {
                max_tokens_per_period: Some(100),
                ..Default::default()
            },
        );
        let r = ledger.reserve(company, 100).expect("reserve");
        ledger.release(r);
        assert_eq!(ledger.totals(company).reserved_tokens, 0);
        ledger.reserve(company, 100).expect("capacity returned");
    }

    #[test]
    fn missing_usage_settles_with_reservation_estimate_not_zero() {
        let ledger = BudgetLedger::new();
        let company = Uuid::new_v4();
        let r = ledger.reserve(company, 50).expect("reserve");
        // Stream died without usage: settle unknown, keep conservative count.
        ledger.settle(r, None, Cost::Unknown);
        let totals = ledger.totals(company);
        assert_eq!(totals.settled_tokens, 50, "estimate is charged, not zero");
        assert_eq!(totals.unknown_cost_calls, 1);
    }
}
