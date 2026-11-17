//! Wire-level `search` tests against a loopback stub of the MemoryLake API.
//!
//! The request body is part of this command's contract: an omitted filter must
//! be absent from the JSON, not `null` and not `[]`, because the API reads an
//! omitted filter as "no restriction".

use serde_json::{Value, json};

use crate::common::stub::{exchange, exchange_sequence, http_response, request_body, request_line};
use crate::common::{assert_failure, assert_success};

const EMPTY_RESULTS: &str = r#"{"success":true,"data":{"documents":[],"facts":[]}}"#;
const NO_PROJECTS: &str = r#"{"success":true,"data":{"items":[]}}"#;
const TWO_PROJECTS: &str =
    r#"{"success":true,"data":{"items":[{"id":"proj-1","name":"a"},{"id":"proj-2","name":"b"}]}}"#;

/// Run a search without `--projects` against a workspace with no projects:
/// the CLI lists them first, then searches. Hands back the search request.
fn search_exchange(results: &str, args: &[&str]) -> (String, std::process::Output) {
    let (mut requests, output) = exchange_sequence(&[NO_PROJECTS, results], args);
    assert!(
        request_line(&requests[0]).starts_with("GET /api/v3/workspaces/"),
        "projects are listed first: {}",
        requests[0]
    );
    (requests.remove(1), output)
}

fn body_of(request: &str) -> Value {
    serde_json::from_str(request_body(request)).expect("request body should be JSON")
}

#[test]
fn query_only_search_posts_just_the_query() {
    let (request, output) = search_exchange(
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
    let (request, _) = search_exchange(
        EMPTY_RESULTS,
        &["search", "--workspace", "ws-1", "anything"],
    );
    let body = body_of(&request);
    let object = body.as_object().expect("object body");
    for key in [
        "project_ids",
        "actor_ids",
        "memory_types",
        "top_k",
        "search_effort",
    ] {
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
            "--effort",
            "low",
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
            "top_k": 5,
            "search_effort": "low"
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
    let (request, _) = search_exchange(
        EMPTY_RESULTS,
        &["search", "--workspace", "ws-1", "  spaced query  "],
    );
    assert_eq!(body_of(&request)["query"], json!("spaced query"));
}

#[test]
fn the_workspace_id_is_encoded_into_its_own_path_segment() {
    let (request, _) = search_exchange(
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
    let (_, output) = search_exchange(
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
fn block_fields_and_rank_reach_the_output() {
    // Paragraph hits carry their text under `highlight.chunks`, not at the top
    // level; none of it may be lost on the way to stdout.
    let (_, output) = search_exchange(
        r#"{"success":true,"data":{"documents":[{"rank":2,"document_id":"doc-1","items":[{"type":"paragraph","paragraph_id":"p-1","highlight":{"chunks":[{"id":"c-1","text":"1.2M","range":"p.4"}]}}]}],"facts":[{"rank":1,"id":"fact-1","score":1.0}]}}"#,
        &["search", "--workspace", "ws-1", "revenue"],
    );
    let stdout = assert_success(&output, &["search"]);
    let parsed: Value = serde_json::from_str(&stdout).expect("stdout should be JSON");

    let item = &parsed["documents"][0]["items"][0];
    assert_eq!(item["type"], "paragraph");
    assert_eq!(item["paragraph_id"], "p-1");
    assert_eq!(item["highlight"]["chunks"][0]["text"], "1.2M");
    assert_eq!(parsed["documents"][0]["rank"], 2);
    assert_eq!(parsed["facts"][0]["rank"], 1);
}

#[test]
fn a_result_set_with_no_matches_still_succeeds() {
    let (_, output) = search_exchange(EMPTY_RESULTS, &["search", "--workspace", "ws-1", "nothing"]);
    let stdout = assert_success(&output, &["search"]);
    let parsed: Value = serde_json::from_str(&stdout).expect("stdout should be JSON");
    assert_eq!(parsed["documents"], json!([]));
    assert_eq!(parsed["facts"], json!([]));
}

#[test]
fn a_response_omitting_both_collections_still_succeeds() {
    // The docs do not say whether empty collections are present or absent.
    let (_, output) = search_exchange(
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
    let (_, output) = search_exchange(
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

#[test]
fn without_projects_every_project_of_the_workspace_is_searched() {
    let args = ["search", "--workspace", "ws-1", "revenue"];
    let (requests, output) = exchange_sequence(&[TWO_PROJECTS, EMPTY_RESULTS], &args);
    assert_success(&output, &args);

    assert_eq!(
        request_line(&requests[0]),
        "GET /api/v3/workspaces/ws-1/projects?page_size=100 HTTP/1.1"
    );
    assert_eq!(
        request_line(&requests[1]),
        "POST /api/v3/workspaces/ws-1/memories/search HTTP/1.1"
    );
    assert_eq!(
        body_of(&requests[1]),
        json!({"query": "revenue", "project_ids": ["proj-1", "proj-2"]})
    );
}

#[test]
fn every_page_of_projects_is_named() {
    let first = r#"{"success":true,"data":{"items":[{"id":"proj-1","name":"a"}],"continuation_token":"t2"}}"#;
    let second = r#"{"success":true,"data":{"items":[{"id":"proj-2","name":"b"}]}}"#;
    let args = ["search", "--workspace", "ws-1", "--types", "database", "q"];
    let (requests, output) = exchange_sequence(&[first, second, EMPTY_RESULTS], &args);
    assert_success(&output, &args);

    assert_eq!(
        request_line(&requests[1]),
        "GET /api/v3/workspaces/ws-1/projects?page_size=100&continuation_token=t2 HTTP/1.1"
    );
    assert_eq!(
        body_of(&requests[2])["project_ids"],
        json!(["proj-1", "proj-2"])
    );
}

#[test]
fn a_facts_only_search_lists_no_projects() {
    // Facts are not scoped by project, so there is nothing to look up.
    let args = ["search", "--workspace", "ws-1", "--types", "fact", "q"];
    let (request, output) = exchange(EMPTY_RESULTS, &args);
    assert_success(&output, &args);
    assert_eq!(
        request_line(&request),
        "POST /api/v3/workspaces/ws-1/memories/search HTTP/1.1"
    );
    assert!(body_of(&request).get("project_ids").is_none());
}

#[test]
fn a_failed_project_listing_fails_the_search() {
    // Searching with no projects instead would quietly drop every document.
    let denied = http_response(
        "403 Forbidden",
        r#"{"success":false,"message":"no access","error_code":"PERMISSION_DENIED"}"#,
    );
    let args = ["search", "--workspace", "ws-1", "q"];
    let (_, output) = exchange_sequence(&[&denied], &args);
    let printed = assert_failure(&output, &args);
    assert!(
        printed.contains("list projects in workspace `ws-1`"),
        "{printed}"
    );
}

#[test]
fn compact_keeps_ids_and_one_snippet_per_hit() {
    let results = r#"{"success":true,"data":{"documents":[{"rank":1,"document_id":"doc-1","project_id":"proj-1","document_name":"Q4.pdf","document_summary":"long","source_type":"file","items":[{"type":"paragraph","highlight":{"chunks":[{"text":"revenue\nwas 1.2M"},{"text":"other"}]}}]}],"facts":[{"rank":2,"id":"fact-1","fact":"Q4 revenue was 1.2M","score":0.5,"metadata":{"k":"v"}}],"databases":[]}}"#;
    let args = [
        "search",
        "--workspace",
        "ws-1",
        "--projects",
        "proj-1",
        "--compact",
        "q",
    ];
    let (_, output) = exchange(results, &args);
    let stdout = assert_success(&output, &args);
    let printed: Value = serde_json::from_str(&stdout).expect("JSON");
    assert_eq!(
        printed,
        json!({
            "documents": [{
                "rank": 1,
                "document_id": "doc-1",
                "project_id": "proj-1",
                "document_name": "Q4.pdf",
                "snippet": "revenue was 1.2M"
            }],
            "facts": [{"rank": 2, "id": "fact-1", "fact": "Q4 revenue was 1.2M"}],
            "databases": []
        })
    );
}
