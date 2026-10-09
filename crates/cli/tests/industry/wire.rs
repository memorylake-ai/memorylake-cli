//! Wire-level `industry` and `project --industry-ids` tests against a stub.

use crate::common::assert_success;
use crate::common::stub::{exchange, request_body, request_line};

const INDUSTRIES: &str = r#"{"success":true,"data":[{"id":"financial/markets","name":"Financial Data","description":"Prices"}]}"#;
const ONE_PROJECT: &str = r#"{"success":true,"data":{"id":"proj-1","name":"P","industries":[]}}"#;

fn body_json(request: &str) -> serde_json::Value {
    serde_json::from_str(request_body(request))
        .unwrap_or_else(|err| panic!("request body is not JSON ({err}): {request}"))
}

#[test]
fn list_calls_the_v1_endpoint_and_prints_the_array() {
    let (request, output) = exchange(INDUSTRIES, &["industry", "list"]);
    let stdout = assert_success(&output, &["industry", "list"]);
    assert_eq!(
        request_line(&request),
        "GET /api/v1/industry-opendata HTTP/1.1"
    );
    let printed: serde_json::Value = serde_json::from_str(&stdout).expect("stdout is JSON");
    assert_eq!(printed[0]["id"], "financial/markets");
}

#[test]
fn project_create_sends_industry_ids_as_an_array() {
    let (request, output) = exchange(
        ONE_PROJECT,
        &[
            "project",
            "create",
            "--workspace",
            "ws-1",
            "--name",
            "P",
            "--custom-id",
            "p-1",
            "--industry-ids",
            "research/academic, financial/markets",
        ],
    );
    assert_success(&output, &["project", "create", "--industry-ids"]);
    assert_eq!(
        body_json(&request),
        serde_json::json!({
            "name": "P",
            "custom_id": "p-1",
            "industry_ids": ["research/academic", "financial/markets"]
        })
    );
}

#[test]
fn project_create_omits_industry_ids_when_not_given() {
    let (request, _) = exchange(
        ONE_PROJECT,
        &[
            "project",
            "create",
            "--workspace",
            "ws-1",
            "--name",
            "P",
            "--custom-id",
            "p-1",
        ],
    );
    assert!(
        !request_body(&request).contains("industry_ids"),
        "{request}"
    );
}

#[test]
fn project_update_replaces_industry_ids() {
    let (request, output) = exchange(
        ONE_PROJECT,
        &[
            "project",
            "update",
            "--workspace",
            "ws-1",
            "proj-1",
            "--industry-ids",
            "clinical/trials",
        ],
    );
    assert_success(&output, &["project", "update", "--industry-ids"]);
    assert_eq!(
        request_line(&request),
        "PATCH /api/v3/workspaces/ws-1/projects/proj-1 HTTP/1.1"
    );
    assert_eq!(
        body_json(&request),
        serde_json::json!({"industry_ids": ["clinical/trials"]})
    );
}

#[test]
fn project_update_clear_industries_sends_an_empty_list() {
    let (request, output) = exchange(
        ONE_PROJECT,
        &[
            "project",
            "update",
            "--workspace",
            "ws-1",
            "proj-1",
            "--clear-industries",
        ],
    );
    assert_success(&output, &["project", "update", "--clear-industries"]);
    assert_eq!(body_json(&request), serde_json::json!({"industry_ids": []}));
}

#[test]
fn project_update_without_industry_flags_leaves_them_alone() {
    let (request, _) = exchange(
        ONE_PROJECT,
        &[
            "project",
            "update",
            "--workspace",
            "ws-1",
            "proj-1",
            "--name",
            "Q",
        ],
    );
    assert_eq!(body_json(&request), serde_json::json!({"name": "Q"}));
}
