//! Wire-level `opendata` and `project --opendata-ids` tests against a stub.

use crate::common::assert_success;
use crate::common::stub::{exchange, request_body, request_line};

const OPENDATA: &str = r#"{"success":true,"data":[{"id":"financial/markets","name":"Financial Data","description":"Prices"}]}"#;
const ONE_PROJECT: &str = r#"{"success":true,"data":{"id":"proj-1","name":"P","industries":[]}}"#;

fn body_json(request: &str) -> serde_json::Value {
    serde_json::from_str(request_body(request))
        .unwrap_or_else(|err| panic!("request body is not JSON ({err}): {request}"))
}

#[test]
fn list_calls_the_v1_endpoint_and_prints_the_array() {
    let (request, output) = exchange(OPENDATA, &["opendata", "list"]);
    let stdout = assert_success(&output, &["opendata", "list"]);
    assert_eq!(
        request_line(&request),
        "GET /api/v1/industry-opendata HTTP/1.1"
    );
    let printed: serde_json::Value = serde_json::from_str(&stdout).expect("stdout is JSON");
    assert_eq!(printed[0]["id"], "financial/markets");
}

#[test]
fn project_create_sends_opendata_ids_as_an_array() {
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
            "--opendata-ids",
            "research/academic, financial/markets",
        ],
    );
    let stdout = assert_success(&output, &["project", "create", "--opendata-ids"]);
    assert_eq!(
        body_json(&request),
        serde_json::json!({
            "name": "P",
            "custom_id": "p-1",
            "industry_ids": ["research/academic", "financial/markets"]
        })
    );
    // The server's `industries` is printed under the CLI's name for it.
    let printed: serde_json::Value = serde_json::from_str(&stdout).expect("stdout is JSON");
    assert!(printed["opendata"].is_array(), "{stdout}");
    assert!(printed.get("industries").is_none(), "{stdout}");
}

#[test]
fn project_create_omits_opendata_ids_when_not_given() {
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
fn project_update_replaces_opendata_ids() {
    let (request, output) = exchange(
        ONE_PROJECT,
        &[
            "project",
            "update",
            "--workspace",
            "ws-1",
            "proj-1",
            "--opendata-ids",
            "clinical/trials",
        ],
    );
    assert_success(&output, &["project", "update", "--opendata-ids"]);
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
fn project_update_clear_opendata_sends_an_empty_list() {
    let (request, output) = exchange(
        ONE_PROJECT,
        &[
            "project",
            "update",
            "--workspace",
            "ws-1",
            "proj-1",
            "--clear-opendata",
        ],
    );
    assert_success(&output, &["project", "update", "--clear-opendata"]);
    assert_eq!(body_json(&request), serde_json::json!({"industry_ids": []}));
}

#[test]
fn project_update_without_opendata_flags_leaves_them_alone() {
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
