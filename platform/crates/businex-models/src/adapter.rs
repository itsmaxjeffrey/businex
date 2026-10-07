//! The adapter contract every provider implements.

use crate::types::{ModelError, ModelRequest, ModelResponse, Price, StreamEvent};
use futures::stream::BoxStream;

#[async_trait::async_trait]
pub trait ModelAdapter: Send + Sync {
    /// Provider identifier (openai, anthropic, gemini, openai-compatible, xiaomi).
    fn provider(&self) -> &str;

    /// Model identifier sent to the provider.
    fn model(&self) -> &str;

    /// Configured price with provenance, or None when the price is unknown.
    fn price(&self) -> Option<&Price>;

    /// One non-streaming completion.
    async fn complete(&self, request: ModelRequest) -> Result<ModelResponse, ModelError>;

    /// Streaming completion; events carry incremental text then a terminal
    /// Completed event with usage when the provider reported it.
    async fn stream(
        &self,
        request: ModelRequest,
    ) -> Result<BoxStream<'static, Result<StreamEvent, ModelError>>, ModelError>;
}
