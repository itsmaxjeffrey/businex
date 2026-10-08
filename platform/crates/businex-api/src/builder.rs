//! Natural language to app manifest: the model-backed builder core.
//!
//! The generator is a seam. Routes depend on the ManifestGenerator trait,
//! production uses the provider adapters from businex-models, and tests
//! substitute a scripted fake. Unsealed provider keys live only inside one
//! GenerateRequest value and are never logged, stored or echoed.

use async_trait::async_trait;
use businex_models::{
    AnthropicAdapter, Cost, GeminiAdapter, Message, ModelAdapter, ModelError, ModelRequest,
    OpenAiAdapter, Price, Role, Usage,
};

/// Providers a company may register keys for and generate with.
pub const PROVIDERS: &[&str] = &[
    "openai",
    "anthropic",
    "gemini",
    "openai-compatible",
    "xiaomi",
];

/// One generation call. api_key is the unsealed provider secret for this
/// request only. This struct deliberately has no Debug or Serialize.
pub struct GenerateRequest {
    pub provider: String,
    pub model: String,
    pub api_key: String,
    /// Trusted endpoint stored with the key under ModelKeysManage.
    /// Generation requests can never override it.
    pub endpoint: Option<String>,
    /// Versioned configured price for this provider/model pair. Absent means
    /// the call's cost is unknown; nothing here is ever guessed.
    pub price: Option<Price>,
    pub description: String,
}

/// What came back from the provider. Usage may be absent and cost may be
/// unknown; both states are recorded honestly rather than filled in.
pub struct GenerateOutcome {
    pub text: String,
    pub usage: Option<Usage>,
    pub cost: Cost,
}

#[async_trait]
pub trait ManifestGenerator: Send + Sync {
    async fn generate(&self, request: GenerateRequest) -> Result<GenerateOutcome, ModelError>;
}

/// Prompt that pins the manifest schema so output parses into AppManifest
/// without repair. Strict parsing is the backstop, so the wording mirrors
/// businex_core::app_manifest exactly.
pub const MANIFEST_SYSTEM_PROMPT: &str = r#"You generate Businex application manifests. Respond with one JSON object only: no prose, no markdown code fences, no comments. The object must match this schema exactly:

{"schema_version":1,"name":"...","slug":"...","kind":"schema","entities":[{"name":"...","fields":[{"name":"...","type":"text","required":true,"unique":false}]}],"permissions":["records.read"],"routes":[{"path":"/things","method":"get","permission":"records.read"}],"schedules":[],"dependencies":[]}

Rules:
- schema_version is always 1.
- name is 1-120 characters. slug is lowercase letters, digits and dashes, starts with a letter, at most 64 characters.
- kind is "schema" for data-driven apps and "code" for custom TypeScript apps.
- A field type is one of "text", "number", "bool", "date". Entity and field names are lowercase identifiers: letters, digits and underscores only.
- permissions use dotted names the platform knows, for example "records.read", "records.write", "records.delete", "files.write", "tasks.manage", "agents.run". Include only what the app needs.
- method is one of "get", "post", "patch", "delete".
- schedules need a name, a five-field cron expression and a handler function name. Use an empty list when the app has none.
- dependencies is a list of npm package names. Use an empty list when the app has none.
- Unknown fields are rejected, so add nothing outside this schema."#;

/// Narrow model output to the outermost JSON object. Tolerates code fences
/// and leading chatter but never repairs the JSON itself: what is returned
/// is parsed strictly afterwards.
pub fn extract_manifest_json(text: &str) -> &str {
    let trimmed = text.trim().trim_matches('\u{60}').trim();
    let trimmed = match trimmed.strip_prefix("json") {
        Some(rest) => rest.trim_start(),
        None => trimmed,
    };
    match (trimmed.find('{'), trimmed.rfind('}')) {
        (Some(start), Some(end)) if end > start => &trimmed[start..=end],
        _ => trimmed,
    }
}

/// Output ceiling sent with every generation request. Kept beside the
/// reservation bound so the two cannot drift apart.
pub const MAX_OUTPUT_TOKENS: u32 = 4096;

/// Conservative pre-dispatch reservation bound: the full request (system
/// prompt plus description) plus the allowed output. Reserving less would
/// let a call commit more than the budget admitted before observed usage
/// lands at settle time.
pub fn estimate_tokens(description: &str) -> i64 {
    // Byte length upper-bounds the token count for any BPE tokenizer (no
    // token is shorter than one byte), so this is a true conservative bound
    // on the request, plus the full allowed output.
    (MANIFEST_SYSTEM_PROMPT.len() + description.len()) as i64 + i64::from(MAX_OUTPUT_TOKENS)
}

/// Production generator: one non-streaming completion per request, priced
/// through the adapter when a price is configured and honestly unknown
/// otherwise.
pub struct ProviderGenerator;

#[async_trait]
impl ManifestGenerator for ProviderGenerator {
    async fn generate(&self, request: GenerateRequest) -> Result<GenerateOutcome, ModelError> {
        let GenerateRequest {
            provider,
            model,
            api_key,
            endpoint,
            price,
            description,
        } = request;
        let adapter = build_adapter(&provider, api_key, &model, endpoint, price)?;
        let model_request = ModelRequest {
            messages: vec![
                Message {
                    role: Role::System,
                    content: MANIFEST_SYSTEM_PROMPT.to_string(),
                },
                Message {
                    role: Role::User,
                    content: description,
                },
            ],
            max_tokens: Some(MAX_OUTPUT_TOKENS),
            temperature: Some(0.2),
        };
        let response = adapter.complete(model_request).await?;
        Ok(GenerateOutcome {
            text: response.text,
            usage: response.usage,
            cost: response.cost,
        })
    }
}

fn build_adapter(
    provider: &str,
    api_key: String,
    model: &str,
    endpoint: Option<String>,
    price: Option<Price>,
) -> Result<Box<dyn ModelAdapter>, ModelError> {
    match provider {
        // The configured price threads through to the adapter so a priced
        // call settles with a real cost; unknown stays unknown, never zero.
        "openai" => Ok(Box::new(OpenAiAdapter::openai(api_key, model, price)?)),
        "anthropic" => Ok(Box::new(AnthropicAdapter::new(api_key, model, price)?)),
        "gemini" => Ok(Box::new(GeminiAdapter::new(api_key, model, price)?)),
        "openai-compatible" => {
            let base = endpoint.ok_or_else(|| {
                ModelError::Config("stored key endpoint is required for openai-compatible".into())
            })?;
            Ok(Box::new(OpenAiAdapter::new(base, api_key, model, price)?))
        }
        "xiaomi" => {
            let base = endpoint.ok_or_else(|| {
                ModelError::Config("stored key endpoint is required for xiaomi".into())
            })?;
            Ok(Box::new(OpenAiAdapter::xiaomi(base, api_key, model, price)?))
        }
        _ => Err(ModelError::Config("unknown provider".into())),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn extraction_returns_plain_object() {
        assert_eq!(extract_manifest_json("{\"a\":1}"), "{\"a\":1}");
    }

    #[test]
    fn extraction_tolerates_fences_and_chatter() {
        let text = "Here you go:\n\u{60}\u{60}\u{60}json\n{\"a\":1}\n\u{60}\u{60}\u{60}\nDone.";
        assert_eq!(extract_manifest_json(text), "{\"a\":1}");
    }

    #[test]
    fn extraction_never_invents_json() {
        assert_eq!(extract_manifest_json("no json here"), "no json here");
    }

    #[test]
    fn estimate_covers_the_whole_request_and_output() {
        let bound = estimate_tokens("");
        // The reserve includes the system prompt and the allowed output,
        // not just a slice of the description.
        assert!(bound > i64::from(MAX_OUTPUT_TOKENS) + 500, "{}", bound);
        assert!(estimate_tokens("more input") > bound);
    }
}
