//! Wire-level `search` tests against a loopback stub of the MemoryLake API.
//!
//! The request body is part of this command's contract: an omitted filter must
//! be absent from the JSON, not `null` and not `[]`, because the API reads an
//! omitted filter as "no restriction".

use serde_json::{Value, json};

use crate::common::assert_success;
use crate::common::stub::{exchange, request_body, request_line};

const EMPTY_RESULTS: &str = r#"{"success":true,"data":{"documents":[],"facts":[]}}"#;

fn body_of(request: &str) -> Value {
    serde_json::from_str(request_body(request)).expect("request body should be JSON")
}

#[test]
fn query_only_search_posts_just_the_query() {
    let (request, output) = exchange(
        EMPTY_RESULTS,
        &["search", "--workspace", "ws-1", "quarterly revenue"],
    );
    assert_success(&output, &["search"]);

    let line = request_line(&request);
    assert_eq!(
        line, "POST /api/v3/workspaces/ws-1/memories/search HTTP/1.1",
        "unexpected request line"
    );
    assert_eq!(body_of(&request), json!({"query": "quarterly revenue"}));
}

#[test]
fn unset_filters_are_absent_not_null_or_empty() {
    let (request, _) = exchange(
        EMPTY_RESULTS,
        &["search", "--workspace", "ws-1", "anything"],
    );
    let body = body_of(&request);
    let object = body.as_object().expect("object body");
    for key in ["project_ids", "actor_ids", "memory_types", "top_k"] {
        assert!(
            !object.contains_key(key),
            "{key} must not be sent when its flag was omitted: {body}"
        );
    }
}

#[test]
fn every_filter_reaches_the_wire_under_its_documented_name() {
    let (request, output) = exchange(
        EMPTY_RESULTS,
        &[
            "search",
            "--workspace",
            "ws-1",
            "--projects",
            "proj-1,proj-2",
            "--actors",
            "act-1",
            "--types",
            "document,fact",
            "--top-k",
            "5",
            "revenue",
        ],
    );
    assert_success(&output, &["search"]);

    assert_eq!(
        body_of(&request),
        json!({
            "query": "revenue",
            "project_ids": ["proj-1", "proj-2"],
            "actor_ids": ["act-1"],
            "memory_types": ["document", "fact"],
            "top_k": 5
        })
    );
}

#[test]
fn comma_lists_are_trimmed_before_they_are_sent() {
    let (request, _) = exchange(
        EMPTY_RESULTS,
        &[
            "search",
            "--workspace",
            "ws-1",
            "--projects",
            " proj-1 , proj-2 ",
            "--types",
            "document, fact",
            "q",
        ],
    );
    let body = body_of(&request);
    assert_eq!(body["project_ids"], json!(["proj-1", "proj-2"]));
    assert_eq!(body["memory_types"], json!(["document", "fact"]));
}

#[test]
fn the_query_is_trimmed_before_it_is_sent() {
    let (request, _) = exchange(
        EMPTY_RESULTS,
        &["search", "--workspace", "ws-1", "  spaced query  "],
    );
    assert_eq!(body_of(&request)["query"], json!("spaced query"));
}

#[test]
fn the_workspace_id_is_encoded_into_its_own_path_segment() {
    let (request, _) = exchange(
        EMPTY_RESULTS,
        &["search", "--workspace", "weird id/here?foo#bar", "q"],
    );
    let line = request_line(&request);
    assert!(
        line.starts_with("POST /api/v3/workspaces/weird%20id%2Fhere%3Ffoo%23bar/memories/search"),
        "workspace id must be percent-encoded: {line}"
    );
}

#[test]
fn results_are_printed_as_pretty_json() {
    let (_, output) = exchange(
        r#"{"success":true,"data":{"documents":[{"document_id":"doc-1","document_name":"Q4","items":[{"text":"1.2M","range":"A1"}]}],"facts":[{"id":"fact-1","fact":"Q4 revenue was 1.2M","score":0.87}]}}"#,
        &["search", "--workspace", "ws-1", "revenue"],
    );
    let stdout = assert_success(&output, &["search"]);

    assert!(stdout.contains('\n'), "output should be pretty-printed");
    let parsed: Value = serde_json::from_str(&stdout).expect("stdout should be JSON");
    assert_eq!(parsed["documents"][0]["document_id"], "doc-1");
    assert_eq!(parsed["documents"][0]["items"][0]["text"], "1.2M");
    assert_eq!(parsed["facts"][0]["score"], 0.87);
}

#[test]
fn a_result_set_with_no_matches_still_succeeds() {
    let (_, output) = exchange(EMPTY_RESULTS, &["search", "--workspace", "ws-1", "nothing"]);
    let stdout = assert_success(&output, &["search"]);
    let parsed: Value = serde_json::from_str(&stdout).expect("stdout should be JSON");
    assert_eq!(parsed["documents"], json!([]));
    assert_eq!(parsed["facts"], json!([]));
}

#[test]
fn a_response_omitting_both_collections_still_succeeds() {
    // The docs do not say whether empty collections are present or absent.
    let (_, output) = exchange(
        r#"{"success":true,"data":{}}"#,
        &["search", "--workspace", "ws-1", "nothing"],
    );
    let stdout = assert_success(&output, &["search"]);
    let parsed: Value = serde_json::from_str(&stdout).expect("stdout should be JSON");
    assert_eq!(parsed["documents"], json!([]));
    assert_eq!(parsed["facts"], json!([]));
}

#[test]
fn server_errors_are_surfaced_verbatim() {
    let (_, output) = exchange(
        r#"{"success":false,"message":"workspace not found","error_code":"WORKSPACE_NOT_FOUND"}"#,
        &["search", "--workspace", "ws-missing", "q"],
    );
    assert!(
        !output.status.success(),
        "a failed search must exit non-zero"
    );
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(stderr.contains("workspace not found"), "{stderr}");
    assert!(stderr.contains("[WORKSPACE_NOT_FOUND]"), "{stderr}");
}
