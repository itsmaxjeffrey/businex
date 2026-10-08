//! Provider-neutral model request/response types.
//!
//! Unknown is a first-class state everywhere:
//! - missing provider usage is Option::None, never zero tokens;
//! - a configured price with unknown usage still computes an unknown cost;
//! - costs use checked arithmetic and reject negative prices.

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Role {
    System,
    User,
    Assistant,
}

impl Role {
    pub fn as_str(self) -> &'static str {
        match self {
            Role::System => "system",
            Role::User => "user",
            Role::Assistant => "assistant",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Message {
    pub role: Role,
    pub content: String,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ModelRequest {
    pub messages: Vec<Message>,
    #[serde(default)]
    pub max_tokens: Option<u32>,
    #[serde(default)]
    pub temperature: Option<f32>,
}

impl ModelRequest {
    pub fn chat(user: impl Into<String>) -> Self {
        ModelRequest {
            messages: vec![Message {
                role: Role::User,
                content: user.into(),
            }],
            max_tokens: None,
            temperature: None,
        }
    }
}

/// Provider-reported usage. Each count is Option: a known zero is different
/// from an absent count. A response with no usage record is usage None.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
pub struct Usage {
    pub input_tokens: Option<u64>,
    pub output_tokens: Option<u64>,
}

impl Usage {
    pub fn of(input_tokens: u64, output_tokens: u64) -> Self {
        Usage {
            input_tokens: Some(input_tokens),
            output_tokens: Some(output_tokens),
        }
    }

    /// Sum when both counts are known; None when any count is absent.
    pub fn total(&self) -> Option<u64> {
        match (self.input_tokens, self.output_tokens) {
            (Some(i), Some(o)) => i.checked_add(o),
            _ => None,
        }
    }

    pub fn is_known(&self) -> bool {
        self.input_tokens.is_some() || self.output_tokens.is_some()
    }
}

/// A configured price with provenance. Prices without primary evidence are
/// recorded as estimates and never presented as authoritative.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Price {
    pub provider: String,
    pub model: String,
    /// ISO 4217 currency code for the micro-unit amounts below.
    pub currency: String,
    /// When this price takes effect (configuration date).
    pub effective_date: String,
    /// True when the price was estimated without primary source evidence.
    pub estimate: bool,
    /// Per million tokens in currency micro-units (1e-6). Must be >= 0.
    pub input_micros_per_mtok: u64,
    pub output_micros_per_mtok: u64,
    /// Optional detail accounting; never double-counted into totals.
    pub cached_input_micros_per_mtok: Option<u64>,
    pub reasoning_micros_per_mtok: Option<u64>,
}

impl Price {
    pub fn validate(&self) -> Result<(), ModelError> {
        if self.currency.len() != 3 {
            return Err(ModelError::Config("currency must be ISO 4217".into()));
        }
        if self.effective_date.len() != 10 {
            return Err(ModelError::Config(
                "effective_date must be YYYY-MM-DD".into(),
            ));
        }
        Ok(())
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "state", rename_all = "lowercase")]
pub enum Cost {
    Known { micros: i64 },
    Unknown,
}

/// Compute cost with checked arithmetic. Unknown usage yields Unknown cost
/// even when a price is configured; overflow or invalid inputs also yield
/// Unknown rather than a wrong number.
pub fn compute_cost(price: Option<&Price>, usage: &Usage) -> Cost {
    let Some(price) = price else {
        return Cost::Unknown;
    };
    let (Some(input), Some(output)) = (usage.input_tokens, usage.output_tokens) else {
        return Cost::Unknown;
    };
    let input_cost = (input as u128)
        .checked_mul(price.input_micros_per_mtok as u128)
        .map(|v| v / 1_000_000);
    let output_cost = (output as u128)
        .checked_mul(price.output_micros_per_mtok as u128)
        .map(|v| v / 1_000_000);
    match (input_cost, output_cost) {
        (Some(i), Some(o)) => match i.checked_add(o).and_then(|t| i64::try_from(t).ok()) {
            Some(micros) if micros >= 0 => Cost::Known { micros },
            _ => Cost::Unknown,
        },
        _ => Cost::Unknown,
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ModelResponse {
    pub provider: String,
    pub model: String,
    pub text: String,
    /// None when the provider reported no usage record at all.
    pub usage: Option<Usage>,
    pub cost: Cost,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "lowercase")]
pub enum StreamEvent {
    Delta {
        text: String,
    },
    /// Terminal event. usage is None when the stream carried no usage record;
    /// callers must then treat the request as unaccounted, not free.
    Completed {
        usage: Option<Usage>,
        ended_with_done: bool,
    },
}

#[derive(Debug, thiserror::Error)]
pub enum ModelError {
    #[error("provider request failed (transport)")]
    Provider,
    /// Sanitized: carries the HTTP status only. Provider error bodies, keys
    /// and prompts never appear in errors or logs.
    #[error("provider rejected the request (status {status})")]
    Rejected { status: u16 },
    #[error("provider stream ended with an error frame")]
    StreamError,
    #[error("configuration error: {0}")]
    Config(String),
    #[error("budget exceeded")]
    BudgetExceeded,
    #[error("transport error")]
    Transport(#[from] reqwest::Error),
    #[error("encoding error")]
    Encoding,
}
