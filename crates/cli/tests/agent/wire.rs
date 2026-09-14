//! Wire-level tests for the A2A `agent` subcommands (`card`, `send`, `task`)
//! against a loopback stub.
//!
//! The stub answers bare A2A JSON, not a MemoryLake envelope, exactly as the
//! production endpoints do. These pin the HTTP method, path, query string,
//! body and headers each subcommand sends, and what it prints back.

use serde_json::Value;

use crate::common::stub::{
    exchange_event_stream_with_remembered_workspace, exchange_with_remembered_workspace,
    request_body, request_header, request_line,
};
use crate::common::{assert_failure, assert_success};

const WS: &str = "ws-1";
const AGENT: &str = "agt-1";
const ROOT: &str = "/api/v3/workspaces/ws-1/agents/agt-1";

const CARD: &str =
    r#"{"protocolVersion":"0.3","name":"Default Agent","capabilities":{"streaming":true}}"#;
const COMPLETED_TASK: &str = r#"{"task":{"id":"run-1","contextId":"ctx-1","status":{"state":"TASK_STATE_COMPLETED"},"artifacts":[{"artifactId":"result-run-1","name":"result","parts":[{"text":"PONG"}]}],"history":[{"role":"ROLE_USER","parts":[{"text":"ping"}]},{"role":"ROLE_AGENT","parts":[{"text":"PONG"}]}]}}"#;
const WORKING_TASK: &str = r#"{"task":{"id":"run-1","contextId":"ctx-1","status":{"state":"TASK_STATE_WORKING"},"history":[]}}"#;
const INPUT_REQUIRED_TASK: &str = r#"{"task":{"id":"run-2","contextId":"ctx-1","status":{"state":"TASK_STATE_INPUT_REQUIRED","message":{"role":"ROLE_AGENT","parts":[{"text":"Which file?"}]}},"artifacts":[]}}"#;
const BARE_TASK: &str = r#"{"id":"run-1","contextId":"ctx-1","status":{"state":"TASK_STATE_COMPLETED"},"metadata":{"task-feedback/v1":{"rating":"up"}}}"#;
const TASK_PAGE: &str = r#"{"tasks":[],"pageSize":5,"totalSize":0}"#;

fn body_json(request: &str) -> Value {
    serde_json::from_str(request_body(request)).expect("request body is JSON")
}

fn stdout_of(output: &std::process::Output) -> String {
    String::from_utf8_lossy(&output.stdout).into_owned()
}

fn stderr_of(output: &std::process::Output) -> String {
    String::from_utf8_lossy(&output.stderr).into_owned()
}

#[test]
fn card_gets_the_well_known_document_and_prints_it_as_is() {
    let args = ["agent", "card", AGENT];
    let (request, output) = exchange_with_remembered_workspace(CARD, WS, &args);
    assert_success(&output, &args);

    assert_eq!(
        request_line(&request),
        format!("GET {ROOT}/.well-known/agent-card.json HTTP/1.1")
    );
    let printed: Value = serde_json::from_str(&stdout_of(&output)).expect("card JSON");
    assert_eq!(printed["name"], "Default Agent");
}

#[test]
fn send_posts_a_v1_message_and_prints_the_reply_text() {
    let args = ["agent", "send", AGENT, "--text", "ping"];
    let (request, output) = exchange_with_remembered_workspace(COMPLETED_TASK, WS, &args);
    assert_success(&output, &args);

    assert_eq!(
        request_line(&request),
        format!("POST {ROOT}/a2a/message:send HTTP/1.1"),
        "v1.0 lives on the unversioned path; `/a2a/v1/` is protocol 0.3"
    );
    let body = body_json(&request);
    assert_eq!(body["message"]["role"], "ROLE_USER");
    assert_eq!(
        body["message"]["parts"],
        serde_json::json!([{"text": "ping"}])
    );
    assert!(
        body["message"]["messageId"]
            .as_str()
            .is_some_and(|id| !id.is_empty()),
        "{body}"
    );
    assert!(
        body.get("configuration").is_none(),
        "default send is blocking: {body}"
    );
    assert!(
        body.get("metadata").is_none(),
        "no scope flags, no metadata: {body}"
    );

    assert_eq!(
        stdout_of(&output),
        "PONG\n",
        "stdout carries only the reply"
    );
    let stderr = stderr_of(&output);
    assert!(stderr.contains("task run-1"), "{stderr}");
    assert!(stderr.contains("context ctx-1"), "{stderr}");
    assert!(stderr.contains("TASK_STATE_COMPLETED"), "{stderr}");
}

#[test]
fn send_maps_scope_flags_onto_the_memorylake_extension() {
    let args = [
        "agent",
        "send",
        AGENT,
        "--text",
        "ping",
        "--context",
        "ctx-9",
        "--task",
        "run-9",
        "--actor",
        "act-1",
        "--project",
        "proj-rw",
        "--read-only-project",
        "proj-a",
        "--read-only-project",
        "proj-b",
        "--skip-memory",
        "--metadata-json",
        r#"{"overrides":{"maxTurns":2}}"#,
        "--raw",
    ];
    let (request, output) = exchange_with_remembered_workspace(COMPLETED_TASK, WS, &args);
    assert_success(&output, &args);

    let body = body_json(&request);
    assert_eq!(body["message"]["contextId"], "ctx-9");
    assert_eq!(body["message"]["taskId"], "run-9");
    assert_eq!(
        body["metadata"]["memorylake"],
        serde_json::json!({
            "actorId": "act-1",
            "readWriteProjectId": "proj-rw",
            "readOnlyProjectIds": ["proj-a", "proj-b"],
            "skipMemory": true,
            "overrides": {"maxTurns": 2}
        })
    );

    let printed: Value = serde_json::from_str(&stdout_of(&output)).expect("--raw prints JSON");
    assert_eq!(printed["task"]["id"], "run-1");
}

#[test]
fn send_no_wait_asks_for_an_immediate_return_and_prints_the_task() {
    let args = ["agent", "send", AGENT, "--text", "ping", "--no-wait"];
    let (request, output) = exchange_with_remembered_workspace(WORKING_TASK, WS, &args);
    assert_success(&output, &args);

    let body = body_json(&request);
    assert_eq!(
        body["configuration"],
        serde_json::json!({"returnImmediately": true})
    );

    let printed: Value = serde_json::from_str(&stdout_of(&output)).expect("task JSON");
    assert_eq!(printed["task"]["status"]["state"], "TASK_STATE_WORKING");
}

#[test]
fn send_explains_how_to_answer_an_input_required_task() {
    let args = ["agent", "send", AGENT, "--text", "open it"];
    let (_, output) = exchange_with_remembered_workspace(INPUT_REQUIRED_TASK, WS, &args);
    assert_success(&output, &args);

    assert_eq!(stdout_of(&output), "Which file?\n");
    let stderr = stderr_of(&output);
    assert!(
        stderr.contains("--task run-2") && stderr.contains("--context ctx-1"),
        "{stderr}"
    );
}

#[test]
fn send_accepts_full_parts_from_json() {
    let args = [
        "agent",
        "send",
        AGENT,
        "--message-json",
        r#"[{"text":"look"},{"url":"https://example.com/x.png","mediaType":"image/png"}]"#,
    ];
    let (request, output) = exchange_with_remembered_workspace(COMPLETED_TASK, WS, &args);
    assert_success(&output, &args);

    let body = body_json(&request);
    assert_eq!(body["message"]["parts"][1]["mediaType"], "image/png");
}

#[test]
fn send_stream_prints_fragments_as_they_arrive_and_the_artifact_only_as_fallback() {
    // The production sequence: task, then token-by-token status updates, then
    // the whole reply again as an artifact, then the terminal status.
    let events = [
        r#"{"task":{"id":"run-1","contextId":"ctx-1","status":{"state":"TASK_STATE_WORKING"}}}"#,
        r#"{"statusUpdate":{"taskId":"run-1","contextId":"ctx-1","status":{"state":"TASK_STATE_WORKING"}}}"#,
        r#"{"statusUpdate":{"taskId":"run-1","contextId":"ctx-1","status":{"state":"TASK_STATE_WORKING","message":{"parts":[{"text":"PO"}]}}}}"#,
        r#"{"statusUpdate":{"taskId":"run-1","contextId":"ctx-1","status":{"state":"TASK_STATE_WORKING","message":{"parts":[{"text":"NG"}]}}}}"#,
        r#"{"artifactUpdate":{"taskId":"run-1","contextId":"ctx-1","lastChunk":true,"artifact":{"parts":[{"text":"PONG"}]}}}"#,
        r#"{"statusUpdate":{"taskId":"run-1","contextId":"ctx-1","status":{"state":"TASK_STATE_COMPLETED"}}}"#,
    ];
    let args = ["agent", "send", AGENT, "--text", "ping", "--stream"];
    let (request, output) = exchange_event_stream_with_remembered_workspace(&events, WS, &args);
    assert_success(&output, &args);

    assert_eq!(
        request_line(&request),
        format!("POST {ROOT}/a2a/message:stream HTTP/1.1")
    );
    assert_eq!(
        request_header(&request, "accept").map(str::to_ascii_lowercase),
        Some("text/event-stream".into()),
        "{request}"
    );

    assert_eq!(
        stdout_of(&output),
        "PONG\n",
        "the artifact repeats the streamed text and must not be printed twice"
    );
    let stderr = stderr_of(&output);
    assert!(
        stderr.contains("task run-1") && stderr.contains("TASK_STATE_COMPLETED"),
        "{stderr}"
    );
}

#[test]
fn send_stream_falls_back_to_the_artifact_when_nothing_was_streamed() {
    let events = [
        r#"{"task":{"id":"run-1","contextId":"ctx-1","status":{"state":"TASK_STATE_WORKING"}}}"#,
        r#"{"artifactUpdate":{"taskId":"run-1","contextId":"ctx-1","artifact":{"parts":[{"text":"PONG"}]}}}"#,
        r#"{"statusUpdate":{"taskId":"run-1","contextId":"ctx-1","status":{"state":"TASK_STATE_COMPLETED"}}}"#,
    ];
    let args = ["agent", "send", AGENT, "--text", "ping", "--stream"];
    let (_, output) = exchange_event_stream_with_remembered_workspace(&events, WS, &args);
    assert_success(&output, &args);
    assert_eq!(stdout_of(&output), "PONG\n");
}

#[test]
fn send_stream_raw_prints_one_event_per_line() {
    let events = [
        r#"{"task":{"id":"run-1"}}"#,
        r#"{"statusUpdate":{"taskId":"run-1","status":{"state":"TASK_STATE_COMPLETED"}}}"#,
    ];
    let args = [
        "agent", "send", AGENT, "--text", "ping", "--stream", "--raw",
    ];
    let (_, output) = exchange_event_stream_with_remembered_workspace(&events, WS, &args);
    assert_success(&output, &args);

    let lines: Vec<Value> = stdout_of(&output)
        .lines()
        .map(|line| serde_json::from_str(line).expect("each line is one JSON event"))
        .collect();
    assert_eq!(lines.len(), 2);
    assert_eq!(lines[0]["task"]["id"], "run-1");
    assert!(stderr_of(&output).is_empty(), "raw mode adds no summary");
}

#[test]
fn task_list_sends_camel_case_query_parameters() {
    let args = [
        "agent",
        "task",
        "list",
        AGENT,
        "--context",
        "ctx-1",
        "--status",
        "TASK_STATE_WORKING",
        "--page-size",
        "5",
        "--page-token",
        "tok",
        "--after",
        "2026-09-01T00:00:00Z",
        "--history-length",
        "2",
        "--artifacts",
    ];
    let (request, output) = exchange_with_remembered_workspace(TASK_PAGE, WS, &args);
    assert_success(&output, &args);

    let line = request_line(&request);
    assert!(
        line.starts_with(&format!("GET {ROOT}/a2a/tasks?")),
        "{line}"
    );
    for expected in [
        "contextId=ctx-1",
        "status=TASK_STATE_WORKING",
        "pageSize=5",
        "pageToken=tok",
        "statusTimestampAfter=2026-09-01T00%3A00%3A00Z",
        "historyLength=2",
        "includeArtifacts=true",
    ] {
        assert!(line.contains(expected), "missing {expected} in {line}");
    }
}

#[test]
fn task_list_without_filters_sends_no_query() {
    let args = ["agent", "task", "list", AGENT];
    let (request, output) = exchange_with_remembered_workspace(TASK_PAGE, WS, &args);
    assert_success(&output, &args);
    assert_eq!(
        request_line(&request),
        format!("GET {ROOT}/a2a/tasks HTTP/1.1")
    );
}

#[test]
fn task_get_activates_the_feedback_extension() {
    let args = [
        "agent",
        "task",
        "get",
        AGENT,
        "run-1",
        "--history-length",
        "3",
    ];
    let (request, output) = exchange_with_remembered_workspace(BARE_TASK, WS, &args);
    assert_success(&output, &args);

    assert_eq!(
        request_line(&request),
        format!("GET {ROOT}/a2a/tasks/run-1?historyLength=3 HTTP/1.1")
    );
    assert_eq!(
        request_header(&request, "a2a-extensions"),
        Some("extensions://task-feedback/v1"),
        "without the header the server hides the rating: {request}"
    );
    let printed: Value = serde_json::from_str(&stdout_of(&output)).expect("task JSON");
    assert_eq!(printed["metadata"]["task-feedback/v1"]["rating"], "up");
}

#[test]
fn task_cancel_posts_to_the_cancel_verb() {
    let args = ["agent", "task", "cancel", AGENT, "run-1"];
    let (request, output) = exchange_with_remembered_workspace(BARE_TASK, WS, &args);
    assert_success(&output, &args);
    assert_eq!(
        request_line(&request),
        format!("POST {ROOT}/a2a/tasks/run-1:cancel HTTP/1.1")
    );
}

#[test]
fn task_feedback_posts_the_rating_with_the_extension_header() {
    let args = [
        "agent",
        "task",
        "feedback",
        AGENT,
        "run-1",
        "--rating",
        "down",
        "--comment",
        "meh",
    ];
    let (request, output) = exchange_with_remembered_workspace(BARE_TASK, WS, &args);
    assert_success(&output, &args);

    assert_eq!(
        request_line(&request),
        format!("POST {ROOT}/a2a/tasks/run-1:feedback HTTP/1.1")
    );
    assert_eq!(
        request_header(&request, "a2a-extensions"),
        Some("extensions://task-feedback/v1"),
        "{request}"
    );
    assert_eq!(
        body_json(&request),
        serde_json::json!({"rating": "down", "comment": "meh"})
    );
}

#[test]
fn task_feedback_without_a_comment_omits_the_field() {
    let args = [
        "agent", "task", "feedback", AGENT, "run-1", "--rating", "up",
    ];
    let (request, output) = exchange_with_remembered_workspace(BARE_TASK, WS, &args);
    assert_success(&output, &args);
    assert_eq!(body_json(&request), serde_json::json!({"rating": "up"}));
}

#[test]
fn an_explicit_workspace_flag_overrides_the_remembered_one() {
    let args = ["agent", "card", AGENT, "--workspace", "ws-other"];
    let (request, output) = exchange_with_remembered_workspace(CARD, WS, &args);
    assert_success(&output, &args);
    assert!(
        request_line(&request).contains("/workspaces/ws-other/"),
        "{}",
        request_line(&request)
    );
}

#[test]
fn an_a2a_error_is_reported_by_its_reason_not_as_escaped_json() {
    // Production's answer to cancelling a finished task: a MemoryLake
    // envelope carrying the A2A error document as a string. The stub answers
    // 200, so this exercises the `success: false` path of the decoder.
    let body = r#"{"success":false,"message":"{\"error\":{\"code\":400,\"status\":\"FAILED_PRECONDITION\",\"message\":\"Task cannot be canceled - current state: 3\",\"details\":[{\"reason\":\"TASK_NOT_CANCELABLE\",\"domain\":\"a2a-protocol.org\"}]}}","error_code":"INVALID_ARGUMENT"}"#;
    let args = ["agent", "task", "cancel", AGENT, "run-1"];
    let (_, output) = exchange_with_remembered_workspace(body, WS, &args);
    let err = assert_failure(&output, &args);
    assert!(
        err.contains("Task cannot be canceled - current state: 3 (TASK_NOT_CANCELABLE)"),
        "{err}"
    );
    let headline = err
        .lines()
        .find(|line| line.contains("Task cannot be canceled"))
        .expect("headline names the failure");
    assert!(
        !headline.contains("\\\"error\\\""),
        "the escaped document must not be the headline: {err}"
    );
}
