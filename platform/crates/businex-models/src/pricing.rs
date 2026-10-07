//! Pricing lookup keyed by provider AND endpoint model name: the same model
//! name on different compatible endpoints can have different prices. Unknown
//! prices are never guessed.

use crate::types::{ModelError, Price};
use std::collections::HashMap;

#[derive(Debug, Clone, Default)]
pub struct PricingTable {
    known: HashMap<(String, String), Price>,
}

impl PricingTable {
    pub fn empty() -> Self {
        PricingTable {
            known: HashMap::new(),
        }
    }

    /// Insert a validated price. Negative values are rejected by u64 types;
    /// malformed provenance is rejected here so it never reaches accounting.
    pub fn insert(&mut self, price: Price) -> Result<(), ModelError> {
        price.validate()?;
        self.known
            .insert((price.provider.clone(), price.model.clone()), price);
        Ok(())
    }

    /// Lookup by provider and model. Unknown means no configured price:
    /// token usage is still tracked, cost stays explicit-unknown.
    pub fn get(&self, provider: &str, model: &str) -> Option<&Price> {
        self.known.get(&(provider.to_string(), model.to_string()))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::types::{compute_cost, Cost, Usage};

    fn price() -> Price {
        Price {
            provider: "openai".into(),
            model: "m".into(),
            currency: "USD".into(),
            effective_date: "2026-01-01".into(),
            estimate: true,
            input_micros_per_mtok: 2_000_000,
            output_micros_per_mtok: 8_000_000,
            cached_input_micros_per_mtok: None,
            reasoning_micros_per_mtok: None,
        }
    }

    #[test]
    fn same_model_name_different_provider_prices_differ() {
        let mut table = PricingTable::empty();
        let mut p2 = price();
        p2.provider = "compatible-gateway".into();
        p2.input_micros_per_mtok = 1;
        table.insert(price()).expect("insert");
        table.insert(p2).expect("insert 2");
        assert_eq!(
            table.get("openai", "m").unwrap().input_micros_per_mtok,
            2_000_000
        );
        assert_eq!(
            table.get("compatible-gateway", "m").unwrap().input_micros_per_mtok,
            1
        );
    }

    #[test]
    fn unknown_price_yields_unknown_cost_not_zero() {
        let table = PricingTable::empty();
        let usage = Usage::of(1_000, 500);
        assert_eq!(compute_cost(table.get("x", "y"), &usage), Cost::Unknown);
    }

    #[test]
    fn known_price_with_unknown_usage_is_unknown_cost() {
        let mut table = PricingTable::empty();
        table.insert(price()).expect("insert");
        let partial = Usage {
            input_tokens: Some(10),
            output_tokens: None,
        };
        assert_eq!(
            compute_cost(table.get("openai", "m"), &partial),
            Cost::Unknown
        );
        let none = Usage::default();
        assert_eq!(compute_cost(table.get("openai", "m"), &none), Cost::Unknown);
    }

    #[test]
    fn known_price_computes_from_both_token_kinds() {
        let mut table = PricingTable::empty();
        table.insert(price()).expect("insert");
        let usage = Usage::of(1_000_000, 250_000);
        let cost = compute_cost(table.get("openai", "m"), &usage);
        assert_eq!(cost, Cost::Known { micros: 4_000_000 });
    }

    #[test]
    fn malformed_price_rejected() {
        let mut table = PricingTable::empty();
        let mut bad = price();
        bad.currency = "dollars".into();
        assert!(table.insert(bad).is_err());
        let mut bad_date = price();
        bad_date.effective_date = "yesterday".into();
        assert!(table.insert(bad_date).is_err());
    }

    #[test]
    fn overflow_yields_unknown_not_wrapped_value() {
        let mut p = price();
        p.input_micros_per_mtok = u64::MAX;
        let usage = Usage::of(u64::MAX, 0);
        assert_eq!(compute_cost(Some(&p), &usage), Cost::Unknown);
    }
}
