//! Wire-level `project` tests against a loopback stub of the API.

use serde_json::Value;

use crate::common::assert_success;
use crate::common::stub::{exchange_with_remembered_workspace, request_line};

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
