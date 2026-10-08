//! OpenAI chat-completions adapter.
//!
//! Also drives OpenAI-compatible endpoints (local models, third-party
//! gateways) and the Xiaomi builder model, which speak the same protocol with
//! a different base URL and key. Custom base URLs are validated against the
//! server network policy before use (https or explicitly allowed loopback for
//! development), so generated app input cannot turn the platform into an
//! internal-network proxy.

use crate::adapter::ModelAdapter;
use crate::types::{
    compute_cost, Cost, Message, ModelError, ModelRequest, ModelResponse, Price, Role, StreamEvent,
    Usage,
};
use crate::urlpolicy::validate_endpoint;
use futures::stream::BoxStream;
use serde_json::{json, Value};

pub struct OpenAiAdapter {
    client: reqwest::Client,
    base_url: String,
    api_key: String,
    model: String,
    price: Option<Price>,
    provider_name: String,
}

impl OpenAiAdapter {
    pub fn new(
        base_url: impl Into<String>,
        api_key: impl Into<String>,
        model: impl Into<String>,
        price: Option<Price>,
    ) -> Result<Self, ModelError> {
        let base_url = base_url.into().trim_end_matches('/').to_string();
        validate_endpoint(&base_url)?;
        Ok(OpenAiAdapter {
            client: crate::urlpolicy::build_client(&base_url),
            base_url,
            api_key: api_key.into(),
            model: model.into(),
            price,
            provider_name: "openai-compatible".into(),
        })
    }

    /// OpenAI itself.
    pub fn openai(
        api_key: impl Into<String>,
        model: impl Into<String>,
        price: Option<Price>,
    ) -> Result<Self, ModelError> {
        let mut a = Self::new("https://api.openai.com/v1", api_key, model, price)?;
        a.provider_name = "openai".into();
        Ok(a)
    }

    /// Xiaomi builder model through its OpenAI-compatible endpoint.
    pub fn xiaomi(
        base_url: impl Into<String>,
        api_key: impl Into<String>,
        model: impl Into<String>,
        price: Option<Price>,
    ) -> Result<Self, ModelError> {
        let mut a = Self::new(base_url, api_key, model, price)?;
        a.provider_name = "xiaomi".into();
        Ok(a)
    }

    fn body(&self, request: &ModelRequest, stream: bool) -> Value {
        let messages: Vec<Value> = request
            .messages
            .iter()
            .map(|m| json!({"role": m.role.as_str(), "content": m.content}))
            .collect();
        let mut body = json!({ "model": self.model, "messages": messages, "stream": stream });
        if let Some(max) = request.max_tokens {
            body["max_tokens"] = json!(max);
        }
        if let Some(t) = request.temperature {
            body["temperature"] = json!(t);
        }
        if stream {
            body["stream_options"] = json!({ "include_usage": true });
        }
        body
    }

    /// Absent usage stays None: provider silence is not zero tokens.
    fn parse_usage(value: &Value) -> Option<Usage> {
        if value.is_null() {
            return None;
        }
        Some(Usage {
            input_tokens: value["prompt_tokens"].as_u64(),
            output_tokens: value["completion_tokens"].as_u64(),
        })
    }

    fn response_cost(&self, usage: &Option<Usage>) -> Cost {
        match usage {
            Some(usage) => compute_cost(self.price.as_ref(), usage),
            None => Cost::Unknown,
        }
    }
}

fn message_text(content: &Value) -> String {
    match content {
        Value::String(s) => s.clone(),
        Value::Array(parts) => parts
            .iter()
            .filter_map(|p| p["text"].as_str())
            .collect::<Vec<_>>()
            .join(""),
        _ => String::new(),
    }
}

fn rejected(status: u16) -> ModelError {
    // Sanitized: the provider body is never propagated to callers or logs.
    ModelError::Rejected { status }
}

#[async_trait::async_trait]
impl ModelAdapter for OpenAiAdapter {
    fn provider(&self) -> &str {
        &self.provider_name
    }

    fn model(&self) -> &str {
        &self.model
    }

    fn price(&self) -> Option<&Price> {
        self.price.as_ref()
    }

    async fn complete(&self, request: ModelRequest) -> Result<ModelResponse, ModelError> {
        let resp = self
            .client
            .post(format!("{}/chat/completions", self.base_url))
            .bearer_auth(&self.api_key)
            .json(&self.body(&request, false))
            .send()
            .await?;
        let status = resp.status().as_u16();
        if !(200..300).contains(&status) {
            return Err(rejected(status));
        }
        let value: Value = resp.json().await?;
        let text = message_text(&value["choices"][0]["message"]["content"]);
        let usage = Self::parse_usage(&value["usage"]);
        Ok(ModelResponse {
            provider: self.provider_name.clone(),
            model: self.model.clone(),
            text,
            cost: self.response_cost(&usage),
            usage,
        })
    }

    async fn stream(
        &self,
        request: ModelRequest,
    ) -> Result<BoxStream<'static, Result<StreamEvent, ModelError>>, ModelError> {
        let resp = self
            .client
            .post(format!("{}/chat/completions", self.base_url))
            .bearer_auth(&self.api_key)
            .json(&self.body(&request, true))
            .send()
            .await?;
        let status = resp.status().as_u16();
        if !(200..300).contains(&status) {
            return Err(rejected(status));
        }
        Ok(crate::sse::parse(
            resp.bytes_stream(),
            |data: &Value, usage: &mut Option<Usage>, out: &mut Vec<StreamEvent>| {
                if let Some(u) = Self::parse_usage(&data["usage"]) {
                    *usage = Some(u);
                }
                let delta = data["choices"][0]["delta"]["content"]
                    .as_str()
                    .unwrap_or("");
                if !delta.is_empty() {
                    out.push(StreamEvent::Delta {
                        text: delta.to_string(),
                    });
                }
            },
        ))
    }
}

/// Convenience for callers that hold a list of chat turns.
pub fn turns_to_messages(turns: Vec<(Role, String)>) -> Vec<Message> {
    turns
        .into_iter()
        .map(|(role, content)| Message { role, content })
        .collect()
}
