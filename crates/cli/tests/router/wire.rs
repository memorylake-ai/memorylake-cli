//! Wire-level `router` tests against a stub.

use crate::common::assert_success;
use crate::common::stub::{
    exchange, exchange_event_stream_with_remembered_workspace, request_body, request_header,
    request_line,
};

const MODELS: &str = r#"{"success":true,"message":"Operation completed successfully","data":{"items":[{"name":"claude-haiku-4-5","provider_type":"anthropic"},{"name":"gpt-4o-mini","provider_type":"openai"}]}}"#;
const CHAT: &str = r#"{"object":"chat.completion","choices":[{"index":0,"message":{"role":"assistant","content":"Lisbon."}}]}"#;
const MESSAGE: &str =
    r#"{"type":"message","role":"assistant","content":[{"type":"text","text":"Lisbon."}]}"#;

fn body_json(request: &str) -> serde_json::Value {
    serde_json::from_str(request_body(request))
        .unwrap_or_else(|err| panic!("request body is not JSON ({err}): {request}"))
}

#[test]
fn models_calls_the_catalog_and_filters_by_protocol() {
    let args = ["router", "models", "--protocol", "anthropic"];
    let (request, output) = exchange(MODELS, &args);
    let stdout = assert_success(&output, &args);
    assert_eq!(request_line(&request), "GET /router/v1/models HTTP/1.1");
    let printed: serde_json::Value = serde_json::from_str(&stdout).expect("stdout is JSON");
    assert_eq!(
        printed,
        serde_json::json!([{"name":"claude-haiku-4-5","provider_type":"anthropic"}])
    );
}

#[test]
fn chat_sends_the_boundary_as_a_query_parameter() {
    let args = [
        "router",
        "chat",
        "Where do I live?",
        "--model",
        "gpt-4o-mini",
        "--boundary",
        "bnd-1",
    ];
    let (request, output) = exchange(CHAT, &args);
    let stdout = assert_success(&output, &args);
    assert_eq!(
        request_line(&request),
        "POST /router/openai/v1/chat/completions?boundary_id=bnd-1 HTTP/1.1"
    );
    assert_eq!(
        body_json(&request),
        serde_json::json!({
            "model": "gpt-4o-mini",
            "messages": [{"role": "user", "content": "Where do I live?"}]
        })
    );
    let printed: serde_json::Value = serde_json::from_str(&stdout).expect("stdout is JSON");
    assert_eq!(printed["choices"][0]["message"]["content"], "Lisbon.");
}

#[test]
fn messages_sends_the_anthropic_version_and_prints_text() {
    let args = [
        "router",
        "messages",
        "hi",
        "--model",
        "claude-haiku-4-5",
        "--text",
    ];
    let (request, output) = exchange(MESSAGE, &args);
    let stdout = assert_success(&output, &args);
    assert_eq!(
        request_line(&request),
        "POST /router/anthropic/v1/messages HTTP/1.1"
    );
    assert_eq!(
        request_header(&request, "anthropic-version"),
        Some("2023-06-01")
    );
    assert_eq!(body_json(&request)["max_tokens"], 1024);
    assert_eq!(stdout, "Lisbon.\n");
}

#[test]
fn chat_stream_prints_the_text_as_it_arrives() {
    let args = ["router", "chat", "hi", "--model", "gpt-4o-mini", "--stream"];
    let (request, output) = exchange_event_stream_with_remembered_workspace(
        &[
            r#"{"choices":[{"delta":{"role":"assistant"}}]}"#,
            r#"{"choices":[{"delta":{"content":"Lis"}}]}"#,
            r#"{"choices":[{"delta":{"content":"bon."}}]}"#,
            "[DONE]",
        ],
        "ws-1",
        &args,
    );
    let stdout = assert_success(&output, &args);
    assert_eq!(body_json(&request)["stream"], true);
    assert_eq!(
        request_header(&request, "accept"),
        Some("text/event-stream")
    );
    assert_eq!(stdout, "Lisbon.\n");
}

#[test]
fn embed_sends_several_texts_as_an_array() {
    let args = [
        "router",
        "embed",
        "a",
        "b",
        "--model",
        "text-embedding-3-small",
    ];
    let (request, output) = exchange(r#"{"object":"list","data":[]}"#, &args);
    assert_success(&output, &args);
    assert_eq!(
        request_line(&request),
        "POST /router/openai/v1/embeddings HTTP/1.1"
    );
    assert_eq!(body_json(&request)["input"], serde_json::json!(["a", "b"]));
}
