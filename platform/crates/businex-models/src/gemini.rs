//! Google Gemini generateContent adapter.

use crate::adapter::ModelAdapter;
use crate::types::{
    compute_cost, Cost, ModelError, ModelRequest, ModelResponse, Price, Role, StreamEvent, Usage,
};
use crate::urlpolicy::validate_endpoint;
use futures::stream::BoxStream;
use serde_json::{json, Value};

pub struct GeminiAdapter {
    client: reqwest::Client,
    base_url: String,
    api_key: String,
    model: String,
    price: Option<Price>,
}

impl GeminiAdapter {
    pub fn new(
        api_key: impl Into<String>,
        model: impl Into<String>,
        price: Option<Price>,
    ) -> Result<Self, ModelError> {
        let base_url = "https://generativelanguage.googleapis.com".to_string();
        Ok(GeminiAdapter {
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

    fn body(&self, request: &ModelRequest) -> Value {
        let mut system = String::new();
        let mut contents = Vec::new();
        for m in &request.messages {
            match m.role {
                Role::System => {
                    if !system.is_empty() {
                        system.push('\n');
                    }
                    system.push_str(&m.content);
                }
                role => contents.push(json!({
                    "role": if role == Role::Assistant { "model" } else { "user" },
                    "parts": [{ "text": m.content }],
                })),
            }
        }
        let mut body = json!({ "contents": contents });
        if !system.is_empty() {
            body["systemInstruction"] = json!({ "parts": [{ "text": system }] });
        }
        let mut generation = json!({});
        if let Some(max) = request.max_tokens {
            generation["maxOutputTokens"] = json!(max);
        }
        if let Some(t) = request.temperature {
            generation["temperature"] = json!(t);
        }
        if generation.as_object().map(|o| !o.is_empty()).unwrap_or(false) {
            body["generationConfig"] = generation;
        }
        body
    }

    fn parse_usage(value: &Value) -> Option<Usage> {
        if value.is_null() {
            return None;
        }
        Some(Usage {
            input_tokens: value["promptTokenCount"].as_u64(),
            output_tokens: value["candidatesTokenCount"].as_u64(),
        })
    }

    fn response_text(value: &Value) -> String {
        value["candidates"][0]["content"]["parts"]
            .as_array()
            .map(|parts| {
                parts
                    .iter()
                    .filter_map(|p| p["text"].as_str())
                    .collect::<Vec<_>>()
                    .join("")
            })
            .unwrap_or_default()
    }
}

fn rejected(status: u16) -> ModelError {
    ModelError::Rejected { status }
}

#[async_trait::async_trait]
impl ModelAdapter for GeminiAdapter {
    fn provider(&self) -> &str {
        "gemini"
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
            .post(format!(
                "{}/v1beta/models/{}:generateContent",
                self.base_url, self.model
            ))
            .header("x-goog-api-key", &self.api_key)
            .json(&self.body(&request))
            .send()
            .await?;
        let status = resp.status().as_u16();
        if !(200..300).contains(&status) {
            return Err(rejected(status));
        }
        let value: Value = resp.json().await?;
        let usage = Self::parse_usage(&value["usageMetadata"]);
        let cost = match usage {
            Some(usage) => compute_cost(self.price.as_ref(), &usage),
            None => Cost::Unknown,
        };
        Ok(ModelResponse {
            provider: "gemini".into(),
            model: self.model.clone(),
            text: Self::response_text(&value),
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
            .post(format!(
                "{}/v1beta/models/{}:streamGenerateContent?alt=sse",
                self.base_url, self.model
            ))
            .header("x-goog-api-key", &self.api_key)
            .json(&self.body(&request))
            .send()
            .await?;
        let status = resp.status().as_u16();
        if !(200..300).contains(&status) {
            return Err(rejected(status));
        }
        Ok(crate::sse::parse(
            resp.bytes_stream(),
            |data: &Value, usage: &mut Option<Usage>, out: &mut Vec<StreamEvent>| {
                let text = Self::response_text(data);
                if !text.is_empty() {
                    out.push(StreamEvent::Delta { text });
                }
                if let Some(u) = Self::parse_usage(&data["usageMetadata"]) {
                    *usage = Some(u);
                }
            },
        ))
    }
}
