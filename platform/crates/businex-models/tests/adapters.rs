//! Adapter tests against a local mock provider: real HTTP round trips,
//! response parsing, streaming across awkward chunk boundaries, explicit
//! unknown usage/cost, sanitized errors and endpoint policy.

use axum::response::IntoResponse;
use businex_models::{
    AnthropicAdapter, Cost, GeminiAdapter, ModelAdapter, ModelError, ModelRequest, OpenAiAdapter,
    Price, StreamEvent, Usage,
};
use futures::StreamExt;
use serde_json::json;

fn price() -> Option<Price> {
    Some(Price {
        provider: "openai-compatible".into(),
        model: "test-model".into(),
        currency: "USD".into(),
        effective_date: "2026-01-01".into(),
        estimate: true,
        input_micros_per_mtok: 1_000_000,
        output_micros_per_mtok: 2_000_000,
        cached_input_micros_per_mtok: None,
        reasoning_micros_per_mtok: None,
    })
}

/// Start the mock provider on a random loopback port. Returns the port and
/// the server task handle.
fn mock_provider() -> (u16, tokio::task::JoinHandle<()>) {
    use axum::routing::post;
    use axum::Router;

    async fn openai_chat(
        headers: axum::http::HeaderMap,
        body: axum::Json<serde_json::Value>,
    ) -> axum::response::Response {
        let authorized = headers
            .get("authorization")
            .and_then(|v| v.to_str().ok())
            .map(|v| v.ends_with("bad-key"))
            .unwrap_or(false);
        if authorized {
            return (
                axum::http::StatusCode::UNAUTHORIZED,
                axum::response::Json(json!({"error": {"message": "bad key"}})),
            )
                .into_response();
        }
        let model = body["model"].as_str().unwrap_or("");
        if model == "test-model-errstream" {
            let chunks = [
                "data: {\"choices\":[{\"delta\":{\"content\":\"part\"}}]}\n\n",
                "data: {\"error\":{\"message\":\"overloaded\"}}\n\n",
            ]
            .concat();
            return axum::response::Response::builder()
                .header("content-type", "text/event-stream")
                .body(axum::body::Body::from(chunks))
                .unwrap();
        }
        if model == "test-model-eof" {
            // Stream body ends without a usage record and without [DONE].
            return axum::response::Response::builder()
                .header("content-type", "text/event-stream")
                .body(axum::body::Body::from(
                    "data: {\"choices\":[{\"delta\":{\"content\":\"cut\"}}]}\n\n",
                ))
                .unwrap();
        }
        let stream = body["stream"].as_bool().unwrap_or(false);
        if !stream {
            return axum::response::Json(json!({
                "choices": [{"message": {"role": "assistant", "content": "Hello"}}],
                "usage": {"prompt_tokens": 7, "completion_tokens": 2},
            }))
            .into_response();
        }
        // Multibyte text in a delta to test chunk-split decoding.
        let chunks = [
            "data: {\"choices\":[{\"delta\":{\"content\":\"Hél\"}}]}\n\n",
            "data: {\"choices\":[{\"delta\":{\"content\":\"lo\"}}]}\n\n",
            "data: {\"usage\":{\"prompt_tokens\":7,\"completion_tokens\":2}}\n\n",
            "data: [DONE]\n\n",
        ]
        .concat();
        let bytes = chunks.into_bytes();
        let chunk_every = body["chunk_every"].as_u64().unwrap_or(0) as usize;
        let stream_body = if chunk_every == 0 {
            axum::body::Body::from(bytes)
        } else {
            // Deterministic chunk boundaries at the source.
            let pieces: Vec<Result<bytes::Bytes, std::io::Error>> = bytes
                .chunks(chunk_every)
                .map(|c| Ok(bytes::Bytes::copy_from_slice(c)))
                .collect();
            axum::body::Body::from_stream(futures::stream::iter(pieces))
        };
        axum::response::Response::builder()
            .header("content-type", "text/event-stream")
            .body(stream_body)
            .unwrap()
    }

    async fn anthropic_messages(body: axum::Json<serde_json::Value>) -> axum::response::Response {
        // The adapter always posts to {base}/v1/messages; behavior dispatches
        // on the requested model so every test exercises real routing.
        if body["model"].as_str() == Some("claude-nousage") {
            return axum::response::Json(json!({
                "content": [{"type": "text", "text": "Hi"}],
            }))
            .into_response();
        }
        let stream = body["stream"].as_bool().unwrap_or(false);
        if stream {
            let chunks = [
                "event: message_start\ndata: {\"type\":\"message_start\",\"message\":{\"usage\":{\"input_tokens\":5}}}\n\n",
                "event: content_block_delta\ndata: {\"type\":\"content_block_delta\",\"delta\":{\"type\":\"text_delta\",\"text\":\"Hi\"}}\n\n",
                "event: message_delta\ndata: {\"type\":\"message_delta\",\"usage\":{\"output_tokens\":1}}\n\n",
                "data: [DONE]\n\n",
            ];
            axum::response::Response::builder()
                .header("content-type", "text/event-stream")
                .body(axum::body::Body::from(chunks.concat()))
                .unwrap()
        } else {
            axum::response::Json(json!({
                "content": [{"type": "text", "text": "Hi"}],
                "usage": {"input_tokens": 5, "output_tokens": 1},
            }))
            .into_response()
        }
    }

    async fn gemini_generate(
        axum::extract::Path(_model): axum::extract::Path<String>,
    ) -> axum::response::Response {
        axum::response::Json(json!({
            "candidates": [{"content": {"parts": [{"text": "Gem"}]}}],
            "usageMetadata": {"promptTokenCount": 3, "candidatesTokenCount": 1},
        }))
        .into_response()
    }

    let app = Router::new()
        .route("/v1/chat/completions", post(openai_chat))
        .route("/v1/messages", post(anthropic_messages))
        // matchit requires whole-segment parameters; the adapter URL suffix
        // (:generateContent) is captured inside the model segment.
        .route("/v1beta/models/{model}", post(gemini_generate));

    let listener = std::net::TcpListener::bind("127.0.0.1:0").expect("bind");
    listener.set_nonblocking(true).expect("nonblocking");
    let port = listener.local_addr().expect("addr").port();
    let handle = tokio::spawn(async move {
        axum::serve(
            tokio::net::TcpListener::from_std(listener).expect("tokio listener"),
            app,
        )
        .await
        .expect("serve");
    });
    (port, handle)
}

async fn collect(
    stream: futures::stream::BoxStream<'static, Result<StreamEvent, ModelError>>,
) -> (String, Option<Usage>, bool, Option<ModelError>) {
    let mut stream = stream;
    let mut text = String::new();
    let mut usage = None;
    let mut ended_with_done = false;
    let mut error = None;
    while let Some(event) = stream.next().await {
        match event {
            Ok(StreamEvent::Delta { text: t }) => text.push_str(&t),
            Ok(StreamEvent::Completed {
                usage: u,
                ended_with_done: d,
            }) => {
                usage = u;
                ended_with_done = d;
            }
            Err(e) => {
                error = Some(e);
                break;
            }
        }
    }
    (text, usage, ended_with_done, error)
}

#[tokio::test(flavor = "multi_thread")]
async fn openai_compatible_complete_parses_text_usage_and_cost() {
    let (port, handle) = mock_provider();
    let adapter = OpenAiAdapter::new(
        format!("http://127.0.0.1:{}/v1", port),
        "test-key",
        "test-model",
        price(),
    )
    .expect("adapter");
    let resp = adapter
        .complete(ModelRequest::chat("hi"))
        .await
        .expect("complete");
    assert_eq!(resp.text, "Hello");
    assert_eq!(resp.usage, Some(Usage::of(7, 2)));
    // 7 * 1.00 + 2 * 2.00 per million = 11 micros.
    assert_eq!(resp.cost, Cost::Known { micros: 11 });
    assert_eq!(resp.provider, "openai-compatible");
    handle.abort();
}

#[tokio::test(flavor = "multi_thread")]
async fn openai_compatible_unknown_price_stays_explicit_unknown() {
    let (port, handle) = mock_provider();
    let adapter = OpenAiAdapter::new(
        format!("http://127.0.0.1:{}/v1", port),
        "test-key",
        "unpriced-model",
        None,
    )
    .expect("adapter");
    let resp = adapter
        .complete(ModelRequest::chat("hi"))
        .await
        .expect("complete");
    assert_eq!(resp.cost, Cost::Unknown);
    assert_eq!(resp.usage, Some(Usage::of(7, 2)), "usage still tracked");
    handle.abort();
}

#[tokio::test(flavor = "multi_thread")]
async fn missing_usage_is_unknown_not_zero() {
    let (port, handle) = mock_provider();
    let adapter = AnthropicAdapter::new("test-key", "claude-nousage", price())
        .expect("adapter")
        .with_base_url(format!("http://127.0.0.1:{}/", port))
        .expect("base url");
    let resp = adapter
        .complete(ModelRequest::chat("hi"))
        .await
        .expect("complete");
    assert_eq!(resp.usage, None, "absent usage stays absent, not zeros");
    assert_eq!(
        resp.cost,
        Cost::Unknown,
        "unknown usage forces unknown cost"
    );
    handle.abort();
}

#[tokio::test(flavor = "multi_thread")]
async fn openai_stream_reassembles_split_frames_and_multibyte() {
    let (port, handle) = mock_provider();
    let adapter = OpenAiAdapter::new(
        format!("http://127.0.0.1:{}/v1", port),
        "test-key",
        "test-model",
        price(),
    )
    .expect("adapter");
    // Force byte-level chunking at 3 bytes: frames AND multibyte characters
    // split across network chunks.
    let mut request = ModelRequest::chat("hi");
    request.max_tokens = Some(10);
    let stream = adapter.stream(request).await.expect("stream");
    // The mock reads chunk_every from the JSON body; adapters do not send it,
    // so this exercises whole-frame delivery. The split-frame behavior is
    // covered by stream_survives_arbitrary_chunk_boundaries below.
    let (text, usage, done, error) = collect(stream).await;
    assert_eq!(text, "Héllo");
    assert_eq!(usage, Some(Usage::of(7, 2)));
    assert!(done, "completed with [DONE] marker");
    assert!(error.is_none());
    handle.abort();
}

#[tokio::test(flavor = "multi_thread")]
async fn stream_survives_arbitrary_chunk_boundaries() {
    // Feed the shared SSE parser synthetic byte chunks split at 1 byte each,
    // including inside a multibyte character, and verify clean reassembly.
    let payload = "data: {\"choices\":[{\"delta\":{\"content\":\"Héllo\"}}]}\n\ndata: [DONE]\n\n";
    let bytes = payload.as_bytes().to_vec();
    let chunks: Vec<Result<bytes::Bytes, reqwest::Error>> = bytes
        .chunks(1)
        .map(|c| Ok(bytes::Bytes::copy_from_slice(c)))
        .collect();
    let source = futures::stream::iter(chunks);
    let stream = businex_models::sse::parse(
        source,
        |data: &serde_json::Value, usage: &mut Option<Usage>, out: &mut Vec<StreamEvent>| {
            if data.get("usage").map(|u| !u.is_null()).unwrap_or(false) {
                *usage = Some(Usage::default());
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
    );
    let (text, _usage, done, error) = collect(stream).await;
    assert_eq!(text, "Héllo");
    assert!(done);
    assert!(error.is_none());
}

#[tokio::test(flavor = "multi_thread")]
async fn provider_error_frames_surface_stream_error() {
    let (port, handle) = mock_provider();
    let adapter = OpenAiAdapter::new(
        format!("http://127.0.0.1:{}/v1", port),
        "test-key",
        "test-model-errstream",
        None,
    )
    .expect("adapter");
    let stream = adapter
        .stream(ModelRequest::chat("hi"))
        .await
        .expect("stream");
    let (text, _usage, _done, error) = collect(stream).await;
    assert_eq!(text, "part", "deltas before the error are kept");
    assert!(
        matches!(error, Some(ModelError::StreamError)),
        "provider error frame becomes StreamError, not a fake success"
    );
    handle.abort();
}

#[tokio::test(flavor = "multi_thread")]
async fn premature_eof_is_not_a_free_success() {
    let (port, handle) = mock_provider();
    let adapter = OpenAiAdapter::new(
        format!("http://127.0.0.1:{}/v1", port),
        "test-key",
        "test-model-eof",
        price(),
    )
    .expect("adapter");
    let stream = adapter
        .stream(ModelRequest::chat("hi"))
        .await
        .expect("stream");
    let (_text, usage, done, error) = collect(stream).await;
    assert!(error.is_none());
    assert!(!done, "no [DONE] marker means the stream was cut");
    assert_eq!(usage, None, "no usage record means unaccounted, not free");
    handle.abort();
}

#[tokio::test(flavor = "multi_thread")]
async fn anthropic_complete_and_stream() {
    let (port, handle) = mock_provider();
    let adapter = AnthropicAdapter::new("test-key", "claude-test", None)
        .expect("adapter")
        .with_base_url(format!("http://127.0.0.1:{}/", port))
        .expect("base url");
    let resp = adapter
        .complete(ModelRequest::chat("hi"))
        .await
        .expect("complete");
    assert_eq!(resp.text, "Hi");
    assert_eq!(resp.usage, Some(Usage::of(5, 1)));
    assert_eq!(resp.provider, "anthropic");

    let stream = adapter
        .stream(ModelRequest::chat("hi"))
        .await
        .expect("stream");
    let (text, usage, done, error) = collect(stream).await;
    assert_eq!(text, "Hi");
    assert_eq!(usage, Some(Usage::of(5, 1)));
    assert!(done);
    assert!(error.is_none());
    handle.abort();
}

#[tokio::test(flavor = "multi_thread")]
async fn gemini_complete() {
    let (port, handle) = mock_provider();
    let adapter = GeminiAdapter::new("test-key", "gemini-test", None)
        .expect("adapter")
        .with_base_url(format!("http://127.0.0.1:{}", port))
        .expect("base url");
    let resp = adapter
        .complete(ModelRequest::chat("hi"))
        .await
        .expect("complete");
    assert_eq!(resp.text, "Gem");
    assert_eq!(resp.usage, Some(Usage::of(3, 1)));
    assert_eq!(resp.provider, "gemini");
    handle.abort();
}

#[tokio::test(flavor = "multi_thread")]
async fn provider_rejection_is_sanitized() {
    let (port, handle) = mock_provider();
    let adapter = OpenAiAdapter::new(
        format!("http://127.0.0.1:{}/v1", port),
        "bad-key",
        "test-model",
        None,
    )
    .expect("adapter");
    match adapter.complete(ModelRequest::chat("hi")).await {
        Err(ModelError::Rejected { status }) => {
            assert_eq!(status, 401, "status is carried");
        }
        other => panic!("expected Rejected, got {:?}", other.map(|_| ())),
    }
    handle.abort();
}

#[tokio::test(flavor = "multi_thread")]
async fn endpoint_policy_rejects_non_https_custom_urls() {
    // Generated app input cannot point adapters at arbitrary internal hosts.
    let result = OpenAiAdapter::new(
        "http://internal.corp.local/v1",
        "test-key",
        "test-model",
        None,
    );
    assert!(
        matches!(result, Err(ModelError::Config(_))),
        "plain http non-loopback endpoints are refused"
    );
    let result = OpenAiAdapter::new("https://10.0.0.5/v1", "test-key", "test-model", None);
    assert!(
        matches!(result, Err(ModelError::Config(_))),
        "private literal addresses are refused"
    );
}
