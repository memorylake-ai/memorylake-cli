//! Wire-level `project` tests against a loopback stub of the API.

use serde_json::Value;

use crate::common::stub::{
    exchange_sequence_with_remembered_workspace, exchange_with_remembered_workspace, request_line,
    workspace_found, workspace_not_found,
};
use crate::common::{assert_failure, assert_success};

const STATISTICS: &str = r#"{"success":true,"data":{"document_count":2,"database_count":0,"document_status":{"pending":0,"running":0,"okay":1,"error":1},"database_status":{"pending":0,"running":0,"okay":0,"error":0}}}"#;

#[test]
fn stats_gets_the_statistics_of_the_project_and_prints_them() {
    let args = ["project", "stats", "proj-1"];
    let (request, output) = exchange_with_remembered_workspace(STATISTICS, "ws-1", &args);
    let stdout = assert_success(&output, &args);

    assert_eq!(
        request_line(&request),
        "GET /api/v3/workspaces/ws-1/projects/proj-1/statistics HTTP/1.1"
    );
    let printed: Value = serde_json::from_str(&stdout).expect("stats JSON");
    assert_eq!(printed["document_count"], 2);
    assert_eq!(printed["document_status"]["error"], 1);
}

#[test]
fn statistics_is_an_alias_of_stats() {
    let args = ["project", "statistics", "proj-1", "--workspace", "ws-2"];
    let (request, output) = exchange_with_remembered_workspace(STATISTICS, "ws-1", &args);
    assert_success(&output, &args);
    assert_eq!(
        request_line(&request),
        "GET /api/v3/workspaces/ws-2/projects/proj-1/statistics HTTP/1.1"
    );
}

const EMPTY_PROJECTS: &str = r#"{"success":true,"data":{"items":[],"continuation_token":null}}"#;

#[test]
fn list_of_a_missing_workspace_fails_with_workspace_not_found() {
    // The list endpoint answers an unknown workspace with an empty page, which
    // reads as "no projects"; the lookup after it tells the two apart.
    let args = ["project", "list", "--workspace", "ws-gone"];
    let (requests, output, _) = exchange_sequence_with_remembered_workspace(
        &[EMPTY_PROJECTS, &workspace_not_found("ws-gone")],
        "ws-1",
        &args,
    );
    let printed = assert_failure(&output, &args);

    assert_eq!(
        request_line(&requests[0]),
        "GET /api/v3/workspaces/ws-gone/projects HTTP/1.1"
    );
    assert_eq!(
        request_line(&requests[1]),
        "GET /api/v3/workspaces/ws-gone HTTP/1.1"
    );
    assert!(
        printed.contains("[WORKSPACE_NOT_FOUND]"),
        "error names the code: {printed}"
    );
    assert!(
        String::from_utf8_lossy(&output.stdout).is_empty(),
        "no empty list is printed"
    );
}

#[test]
fn empty_list_of_an_existing_workspace_still_prints() {
    let args = ["project", "list"];
    let (requests, output, _) = exchange_sequence_with_remembered_workspace(
        &[EMPTY_PROJECTS, &workspace_found("ws-1")],
        "ws-1",
        &args,
    );
    let stdout = assert_success(&output, &args);

    assert_eq!(
        request_line(&requests[1]),
        "GET /api/v3/workspaces/ws-1 HTTP/1.1"
    );
    let printed: Value = serde_json::from_str(&stdout).expect("list JSON");
    assert_eq!(printed["items"], Value::Array(Vec::new()));
}

#[test]
fn non_empty_list_costs_no_workspace_lookup() {
    // The stub stops listening after its one response, so a lookup would be
    // refused and logged at debug level; `-vv` makes that log visible.
    let page = r#"{"success":true,"data":{"items":[{"id":"proj-1","name":"p"}]}}"#;
    let args = ["project", "list", "-vv"];
    let (requests, output, _) = exchange_sequence_with_remembered_workspace(&[page], "ws-1", &args);
    assert_success(&output, &args);
    assert_eq!(requests.len(), 1);
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        !stderr.contains("workspace lookup"),
        "no workspace lookup after a non-empty page: {stderr}"
    );
}

#[test]
fn list_compact_keeps_id_name_and_custom_id() {
    let page = r#"{"success":true,"data":{"items":[{"id":"proj-1","name":"p","custom_id":"c-1","description":"long","opendata":[],"metadata":{"k":"v"},"workspace_id":"ws-1","created_at":"2026-11-01T00:00:00Z","updated_at":"2026-11-02T00:00:00Z"}],"continuation_token":"next"}}"#;
    let args = ["project", "list", "--compact"];
    let (_, output, _) = exchange_sequence_with_remembered_workspace(&[page], "ws-1", &args);
    let stdout = assert_success(&output, &args);
    let printed: Value = serde_json::from_str(&stdout).expect("list JSON");
    assert_eq!(
        printed,
        serde_json::json!({
            "items": [{"id": "proj-1", "name": "p", "custom_id": "c-1", "updated_at": "2026-11-02T00:00:00Z"}],
            "continuation_token": "next"
        })
    );
}
