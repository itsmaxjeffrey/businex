//! Anthropic Messages API adapter.

use crate::adapter::ModelAdapter;
use crate::types::{
    compute_cost, Cost, ModelError, ModelRequest, ModelResponse, Price, Role, StreamEvent, Usage,
};
use crate::urlpolicy::validate_endpoint;
use futures::stream::BoxStream;
use serde_json::{json, Value};

pub struct AnthropicAdapter {
    client: reqwest::Client,
    base_url: String,
    api_key: String,
    model: String,
    price: Option<Price>,
}

impl AnthropicAdapter {
    pub fn new(
        api_key: impl Into<String>,
        model: impl Into<String>,
        price: Option<Price>,
    ) -> Result<Self, ModelError> {
        let base_url = "https://api.anthropic.com".to_string();
        Ok(AnthropicAdapter {
            client: crate::urlpolicy::build_client(&base_url),
            base_url,
            api_key: api_key.into(),
            model: model.into(),
            price,
        })
    }

    pub fn with_base_url(mut self, base_url: impl Into<String>) -> Result<Self, ModelError> {
        let base_url = base_url.into().trim_end_matches('/').to_string();
        validate_endpoint(&base_url)?;
        self.client = crate::urlpolicy::build_client(&base_url);
        self.base_url = base_url;
        Ok(self)
    }

    fn body(&self, request: &ModelRequest, stream: bool) -> Value {
        let mut system = String::new();
        let mut messages = Vec::new();
        for m in &request.messages {
            match m.role {
                Role::System => {
                    if !system.is_empty() {
                        system.push('\n');
                    }
                    system.push_str(&m.content);
                }
                Role::User => messages.push(json!({"role": "user", "content": m.content})),
                Role::Assistant => messages.push(json!({"role": "assistant", "content": m.content})),
            }
        }
        let mut body = json!({
            "model": self.model,
            "messages": messages,
            "max_tokens": request.max_tokens.unwrap_or(4096),
            "stream": stream,
        });
        if !system.is_empty() {
            body["system"] = json!(system);
        }
        if let Some(t) = request.temperature {
            body["temperature"] = json!(t);
        }
        body
    }

    fn parse_usage(value: &Value) -> Option<Usage> {
        if value.is_null() {
            return None;
        }
        Some(Usage {
            input_tokens: value["input_tokens"].as_u64(),
            output_tokens: value["output_tokens"].as_u64(),
        })
    }
}

fn rejected(status: u16) -> ModelError {
    // Sanitized: no provider body, keys or prompts in errors.
    ModelError::Rejected { status }
}

#[async_trait::async_trait]
impl ModelAdapter for AnthropicAdapter {
    fn provider(&self) -> &str {
        "anthropic"
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
            .post(format!("{}/v1/messages", self.base_url))
            .header("x-api-key", &self.api_key)
            .header("anthropic-version", "2023-06-01")
            .json(&self.body(&request, false))
            .send()
            .await?;
        let status = resp.status().as_u16();
        if !(200..300).contains(&status) {
            return Err(rejected(status));
        }
        let value: Value = resp.json().await?;
        let text = value["content"]
            .as_array()
            .map(|blocks| {
                blocks
                    .iter()
                    .filter(|b| b["type"] == "text")
                    .filter_map(|b| b["text"].as_str())
                    .collect::<Vec<_>>()
                    .join("")
            })
            .unwrap_or_default();
        let usage = Self::parse_usage(&value["usage"]);
        let cost = match usage {
            Some(usage) => compute_cost(self.price.as_ref(), &usage),
            None => Cost::Unknown,
        };
        Ok(ModelResponse {
            provider: "anthropic".into(),
            model: self.model.clone(),
            text,
            cost,
            usage,
        })
    }

    async fn stream(
        &self,
        request: ModelRequest,
    ) -> Result<BoxStream<'static, Result<StreamEvent, ModelError>>, ModelError> {
        let resp = self
            .client
            .post(format!("{}/v1/messages", self.base_url))
            .header("x-api-key", &self.api_key)
            .header("anthropic-version", "2023-06-01")
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
                match data["type"].as_str() {
                    Some("content_block_delta") => {
                        if let Some(text) = data["delta"]["text"].as_str() {
                            if !text.is_empty() {
                                out.push(StreamEvent::Delta {
                                    text: text.to_string(),
                                });
                            }
                        }
                    }
                    Some("message_start") => {
                        let mut u = usage.unwrap_or_default();
                        u.input_tokens = data["message"]["usage"]["input_tokens"].as_u64();
                        *usage = Some(u);
                    }
                    Some("message_delta") => {
                        let mut u = usage.unwrap_or_default();
                        u.output_tokens = data["usage"]["output_tokens"].as_u64();
                        *usage = Some(u);
                    }
                    _ => {}
                }
            },
        ))
    }
}
