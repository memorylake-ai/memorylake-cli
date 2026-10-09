//! Wire-level `project database` tests against a loopback stub of the API.

use serde_json::{Value, json};

use crate::common::stub::{
    exchange_raw_event_stream_with_remembered_workspace, exchange_with_input,
    exchange_with_remembered_workspace, logged_in_home, request_body, request_header, request_line,
};
use crate::common::{assert_failure, assert_success, run_with_input};

const WS: &str = "ws-remembered";
const BASE: &str = "/api/v3/workspaces/ws-remembered/projects/proj-1/memories/databases";
const MEMORY: &str = r#"{"success":true,"data":{"id":"db-1","name":"analytics_db","status":"okay","db_datasource_id":"ds-1","external_id":"legacy"}}"#;

fn body_json(request: &str) -> Value {
    serde_json::from_str(request_body(request)).expect("request body is JSON")
}

#[test]
fn list_sends_paging_params() {
    let args = [
        "project",
        "database",
        "list",
        "--project",
        "proj-1",
        "--page-size",
        "10",
        "--continuation-token",
        "tok",
    ];
    let (request, output) = exchange_with_remembered_workspace(
        r#"{"success":true,"data":{"items":[],"total":0}}"#,
        WS,
        &args,
    );
    let stdout = assert_success(&output, &args);
    assert_eq!(
        request_line(&request),
        format!("GET {BASE}?page_size=10&continuation_token=tok HTTP/1.1")
    );
    assert_eq!(
        serde_json::from_str::<Value>(&stdout).unwrap(),
        json!({"items": [], "total": 0})
    );
}

#[test]
fn create_sends_the_documented_body() {
    let args = [
        "proj",
        "db",
        "create",
        "--project",
        "proj-1",
        "--datasource",
        "ds-1",
        "--name",
        "analytics_db",
        "--instruction",
        "Amounts are in cents.",
        "--analysis-model",
        "am-1",
    ];
    let (request, output) = exchange_with_remembered_workspace(MEMORY, WS, &args);
    let stdout = assert_success(&output, &args);
    assert_eq!(request_line(&request), format!("POST {BASE} HTTP/1.1"));
    assert_eq!(
        body_json(&request),
        json!({"name": "analytics_db", "db_datasource_id": "ds-1",
               "instruction": "Amounts are in cents.", "analysis_model_id": "am-1"})
    );
    // Unmodeled fields are printed, not dropped.
    assert!(stdout.contains("\"external_id\": \"legacy\""), "{stdout}");
}

#[test]
fn update_reads_the_instruction_from_stdin() {
    let args = [
        "project",
        "database",
        "update",
        "--workspace",
        WS,
        "--project",
        "proj-1",
        "db-1",
        "--instruction-file",
        "-",
        "--clear-analysis-model",
    ];
    let (request, output) = exchange_with_input(MEMORY, &args, "Line one.\nLine two.\n", &[]);
    assert_success(&output, &args);
    assert_eq!(
        request_line(&request),
        format!("PATCH {BASE}/db-1 HTTP/1.1")
    );
    // One trailing newline is dropped, inner ones kept; clearing the model
    // sends "".
    assert_eq!(
        body_json(&request),
        json!({"instruction": "Line one.\nLine two.", "analysis_model_id": ""})
    );
}

#[test]
fn clear_instruction_sends_an_empty_instruction() {
    let args = [
        "project",
        "database",
        "update",
        "--project",
        "proj-1",
        "db-1",
        "--clear-instruction",
    ];
    let (request, output) = exchange_with_remembered_workspace(MEMORY, WS, &args);
    assert_success(&output, &args);
    assert_eq!(body_json(&request), json!({"instruction": ""}));
}

#[test]
fn get_delete_reload_address_one_memory() {
    for (args, response, line) in [
        (
            vec!["project", "database", "get", "--project", "proj-1", "db-1"],
            MEMORY,
            format!("GET {BASE}/db-1 HTTP/1.1"),
        ),
        (
            vec![
                "project",
                "database",
                "delete",
                "--project",
                "proj-1",
                "db-1",
            ],
            r#"{"success":true}"#,
            format!("DELETE {BASE}/db-1 HTTP/1.1"),
        ),
        (
            vec![
                "project",
                "database",
                "reload",
                "--project",
                "proj-1",
                "db-1",
            ],
            r#"{"success":true,"data":{}}"#,
            format!("POST {BASE}/db-1/reload HTTP/1.1"),
        ),
    ] {
        let (request, output) = exchange_with_remembered_workspace(response, WS, &args);
        let stdout = assert_success(&output, &args);
        assert_eq!(request_line(&request), line, "{args:?}");
        if args[2] == "delete" {
            assert!(
                stdout.contains("Deleted database memory `db-1` from project `proj-1`"),
                "{stdout}"
            );
        }
        if args[2] == "reload" {
            assert!(stdout.contains("schema reload"), "{stdout}");
        }
    }
}

#[test]
fn generate_instruction_streams_and_prints_the_draft() {
    let body = ": keepalive\n\n\
                event: data\ndata: {\"instruction\":\"Revenue is in cents.\"}\n\n\
                event: end\ndata: {}\n\n";
    let args = [
        "project",
        "database",
        "generate-instruction",
        "--project",
        "proj-1",
        "db-1",
    ];
    let (request, output) = exchange_raw_event_stream_with_remembered_workspace(body, WS, &args);
    let stdout = assert_success(&output, &args);
    assert_eq!(
        request_line(&request),
        format!("POST {BASE}/db-1/instruction/generate HTTP/1.1")
    );
    assert_eq!(body_json(&request), json!({"stream": true}));
    assert_eq!(
        request_header(&request, "accept"),
        Some("text/event-stream")
    );
    assert_eq!(
        serde_json::from_str::<Value>(&stdout).unwrap(),
        json!({"instruction": "Revenue is in cents."})
    );
}

#[test]
fn a_generation_error_event_fails_with_its_code() {
    let body = ": keepalive\n\nevent: error\ndata: {\"error_code\":\"LLM_UNAVAILABLE\",\"message\":\"model busy\"}\n\n";
    let args = [
        "project",
        "database",
        "generate-instruction",
        "--project",
        "proj-1",
        "db-1",
    ];
    let (_, output) = exchange_raw_event_stream_with_remembered_workspace(body, WS, &args);
    let output = assert_failure(&output, &args);
    assert!(output.contains("LLM_UNAVAILABLE"), "{output}");
    assert!(output.contains("model busy"), "{output}");
}

#[test]
fn a_generation_error_event_without_json_keeps_the_raw_text() {
    let body = "event: error\ndata: upstream timed out\n\n";
    let args = [
        "project",
        "database",
        "generate-instruction",
        "--project",
        "proj-1",
        "db-1",
    ];
    let (_, output) = exchange_raw_event_stream_with_remembered_workspace(body, WS, &args);
    let output = assert_failure(&output, &args);
    assert!(output.contains("upstream timed out"), "{output}");
}

#[test]
fn a_json_envelope_from_generate_is_accepted() {
    // A server that ignores `stream: true` answers with a plain envelope.
    let args = [
        "project",
        "database",
        "generate-instruction",
        "--project",
        "proj-1",
        "db-1",
    ];
    let (_, output) = exchange_with_remembered_workspace(
        r#"{"success":true,"data":{"instruction":"Revenue is in cents."}}"#,
        WS,
        &args,
    );
    let stdout = assert_success(&output, &args);
    assert_eq!(
        serde_json::from_str::<Value>(&stdout).unwrap(),
        json!({"instruction": "Revenue is in cents."})
    );
}

#[test]
fn a_blank_instruction_file_is_refused_before_anything_is_sent() {
    // `generate-instruction ... | update --instruction-file -` with a failed
    // first command pipes nothing; that must not wipe the saved instruction.
    let home = logged_in_home("http://127.0.0.1:9");
    let args = [
        "project",
        "database",
        "update",
        "--project",
        "p",
        "db-1",
        "--workspace",
        "ws-1",
        "--instruction-file",
        "-",
    ];
    for stdin in ["", "\n", "  \n\t\n"] {
        let output = assert_failure(&run_with_input(&home, &args, stdin, &[]), &args);
        assert!(output.contains("instruction file is empty"), "{output}");
        assert!(output.contains("--clear-instruction"), "{output}");
    }
    let _ = std::fs::remove_dir_all(&home);
}

#[test]
fn blank_values_are_refused_by_the_parser() {
    let home = logged_in_home("http://127.0.0.1:9");
    for args in [
        vec![
            "project",
            "database",
            "update",
            "--project",
            "p",
            "db-1",
            "--instruction",
            "",
        ],
        vec![
            "project",
            "database",
            "update",
            "--project",
            "p",
            "db-1",
            "--analysis-model",
            "",
        ],
        vec![
            "project",
            "database",
            "create",
            "--project",
            "p",
            "--datasource",
            "ds-1",
            "--name",
            " ",
        ],
    ] {
        let output = assert_failure(&run_with_input(&home, &args, "", &[]), &args);
        assert!(output.contains("must not be empty"), "{args:?}: {output}");
    }
    let _ = std::fs::remove_dir_all(&home);
}

#[test]
fn clear_instruction_conflicts_with_a_new_one() {
    let home = logged_in_home("http://127.0.0.1:9");
    let args = [
        "project",
        "database",
        "update",
        "--project",
        "p",
        "db-1",
        "--clear-instruction",
        "--instruction",
        "x",
    ];
    let output = assert_failure(&run_with_input(&home, &args, "", &[]), &args);
    assert!(output.contains("cannot be used with"), "{output}");
    let _ = std::fs::remove_dir_all(&home);
}
