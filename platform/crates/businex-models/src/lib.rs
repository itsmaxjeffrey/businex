//! Model adapters and usage accounting for the Businex platform.
//!
//! Providers: OpenAI, Anthropic, Gemini, OpenAI-compatible endpoints (local
//! models, gateways) and the Xiaomi builder model. Per-company keys are stored
//! sealed (AES-256-GCM, tenant/provider bound) and unsealed only for the
//! duration of a call. Every response records token usage as reported (or
//! explicitly unknown) and a cost that is explicit about unknown prices.
//! Budgets reserve capacity before calls dispatch.

pub mod adapter;
pub mod anthropic;
pub mod budget;
pub mod gemini;
pub mod keys;
pub mod openai;
pub mod pricing;
pub mod sse;
pub mod store;
pub mod types;
pub mod urlpolicy;

pub use adapter::ModelAdapter;
pub use anthropic::AnthropicAdapter;
pub use budget::{Budget, BudgetDecision, BudgetLedger, Reservation, UsageTotals};
pub use gemini::GeminiAdapter;
pub use keys::{KeyError, MasterKey, SealedKey};
pub use openai::OpenAiAdapter;
pub use pricing::PricingTable;
pub use store::{
    get_budget, release, reserve, set_budget, settle, BudgetRow, DenyReason, StoreError,
};
pub use types::{
    compute_cost, Cost, Message, ModelError, ModelRequest, ModelResponse, Price, Role, StreamEvent,
    Usage,
};
