//! Wire-level tests for `boundary` against a loopback stub.

use serde_json::Value;

use crate::common::assert_success;
use crate::common::stub::{
    exchange, exchange_with_remembered_workspace, request_body, request_line,
};

const BOUNDARY: &str = r#"{"success":true,"data":{"id":"bnd-1","name":"scope","workspace_id":"ws-1","agent_id":"agent-1","assistant_actor_id":"actor-9"}}"#;
const EMPTY_PAGE: &str = r#"{"success":true,"data":{"items":[]}}"#;

fn body_json(request: &str) -> Value {
    serde_json::from_str(request_body(request)).expect("request body is JSON")
}

#[test]
fn list_uses_the_remembered_workspace_as_a_query_parameter() {
    let args = ["boundary", "list", "--page-size", "5", "--name", "scope"];
    let (request, output) = exchange_with_remembered_workspace(EMPTY_PAGE, "ws-1", &args);
    assert_success(&output, &args);
    assert_eq!(
        request_line(&request),
        "GET /api/v3/boundaries?workspace_id=ws-1&page_size=5&name_fuzzy=scope HTTP/1.1"
    );
}

#[test]
fn create_sends_the_workspace_and_every_scope() {
    let args = [
        "boundary",
        "create",
        "--name",
        "scope",
        "--custom-id",
        "c-1",
        "--projects",
        "proj-1, proj-2",
        "--human-actor",
        "actor-1",
        "--agent",
        "agent-1",
    ];
    let (request, output) = exchange_with_remembered_workspace(BOUNDARY, "ws-1", &args);
    let stdout = assert_success(&output, &args);
    assert_eq!(request_line(&request), "POST /api/v3/boundaries HTTP/1.1");
    assert_eq!(
        body_json(&request),
        serde_json::json!({
            "name": "scope",
            "workspace_id": "ws-1",
            "custom_id": "c-1",
            "project_ids": ["proj-1", "proj-2"],
            "human_actor_id": "actor-1",
            "agent_id": "agent-1"
        })
    );
    let printed: Value = serde_json::from_str(&stdout).expect("stdout JSON");
    assert_eq!(
        printed["assistant_actor_id"], "actor-9",
        "undocumented fields survive"
    );
}

#[test]
fn create_with_an_explicit_workspace_sends_only_the_name() {
    let args = [
        "boundary",
        "create",
        "--workspace",
        "ws-2",
        "--name",
        "scope",
    ];
    let (request, output) = exchange(BOUNDARY, &args);
    assert_success(&output, &args);
    assert_eq!(
        body_json(&request),
        serde_json::json!({"name": "scope", "workspace_id": "ws-2"})
    );
}

#[test]
fn get_by_custom_id_sets_the_query_flag() {
    let args = ["boundary", "get", "c-1", "--by-custom-id"];
    let (request, output) = exchange(BOUNDARY, &args);
    assert_success(&output, &args);
    assert_eq!(
        request_line(&request),
        "GET /api/v3/boundaries/c-1?by_custom_id=true HTTP/1.1"
    );

    let args = ["boundary", "get", "bnd-1"];
    let (request, output) = exchange(BOUNDARY, &args);
    assert_success(&output, &args);
    assert_eq!(
        request_line(&request),
        "GET /api/v3/boundaries/bnd-1 HTTP/1.1"
    );
}

#[test]
fn update_sends_values_and_spells_clears_the_way_the_api_does() {
    let args = [
        "boundary",
        "update",
        "bnd-1",
        "--name",
        "renamed",
        "--clear-projects",
        "--clear-human-actor",
        "--agent",
        "agent-2",
    ];
    let (request, output) = exchange(BOUNDARY, &args);
    assert_success(&output, &args);
    assert_eq!(
        request_line(&request),
        "PATCH /api/v3/boundaries/bnd-1 HTTP/1.1"
    );
    assert_eq!(
        body_json(&request),
        serde_json::json!({
            "name": "renamed",
            "project_ids": [],
            "human_actor_id": "",
            "agent_id": "agent-2"
        })
    );

    let args = ["boundary", "update", "bnd-1", "--clear-agent"];
    let (request, output) = exchange(BOUNDARY, &args);
    assert_success(&output, &args);
    assert_eq!(body_json(&request), serde_json::json!({"agent_id": ""}));
}

#[test]
fn delete_prints_a_confirmation() {
    let args = ["boundary", "delete", "bnd-1"];
    let (request, output) = exchange(r#"{"success":true}"#, &args);
    let stdout = assert_success(&output, &args);
    assert_eq!(
        request_line(&request),
        "DELETE /api/v3/boundaries/bnd-1 HTTP/1.1"
    );
    assert!(stdout.contains("Deleted boundary `bnd-1`"), "{stdout}");
}
