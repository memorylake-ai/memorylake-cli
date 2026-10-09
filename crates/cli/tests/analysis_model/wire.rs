//! Wire-level `analysis-model` tests against a loopback stub of the API.
//!
//! These pin the method, path, query string and body each subcommand sends,
//! and what it prints back. The create/entry success paths cannot be proven
//! live (a model needs a real database datasource), so these are what holds
//! the documented request shapes in place.

use std::fs;

use crate::common::stub::{
    exchange, exchange_with_remembered_workspace, request_body, request_line,
};
use crate::common::{assert_success, unique_name};

const BASE: &str = "/api/v3/workspaces/ws-1/analysis-models";

const EMPTY_PAGE: &str = r#"{"success":true,"data":{"items":[]}}"#;
const ONE_MODEL: &str = r#"{"success":true,"data":{"id":"am-1","name":"Sales","type":"ASK_DATA","db_datasource_id":"ds-1","schemas":["public"],"future_field":"kept"}}"#;
const ONE_ENTRY: &str = r#"{"success":true,"data":{"id":"e-1","entity_type":"few_shot","disabled":false,"extra":{"owner":"team"},"payload":{"artifact":{}}}}"#;
const NO_DATA: &str = r#"{"success":true,"message":"Operation completed successfully"}"#;
const EMPTY_DATA: &str = r#"{"success":true,"data":{}}"#;

/// The request body as JSON.
fn body_json(request: &str) -> serde_json::Value {
    serde_json::from_str(request_body(request))
        .unwrap_or_else(|err| panic!("request body is not JSON ({err}): {request}"))
}

#[test]
fn templates_calls_the_templates_endpoint() {
    let (request, output) = exchange(
        r#"{"success":true,"data":[{"type":"ASK_DATA","entity_types":[{"code":"few_shot"}]}]}"#,
        &[
            "analysis-model",
            "templates",
            "--workspace",
            "ws-1",
            "--type",
            "ASK_DATA",
        ],
    );
    let stdout = assert_success(&output, &["analysis-model", "templates"]);
    assert_eq!(
        request_line(&request),
        format!("GET {BASE}/templates?type=ASK_DATA HTTP/1.1")
    );
    assert!(stdout.contains("\"few_shot\""), "{stdout}");
}

#[test]
fn list_sends_every_filter() {
    let (request, output) = exchange(
        EMPTY_PAGE,
        &[
            "am",
            "list",
            "--workspace",
            "ws-1",
            "--datasource",
            "ds-1",
            "--type",
            "ASK_DATA",
            "--page-size",
            "5",
            "--continuation-token",
            "tok",
        ],
    );
    assert_success(&output, &["am", "list"]);
    assert_eq!(
        request_line(&request),
        format!(
            "GET {BASE}?db_datasource_id=ds-1&type=ASK_DATA&page_size=5&continuation_token=tok HTTP/1.1"
        )
    );
}

#[test]
fn list_uses_the_remembered_workspace() {
    let (request, output) =
        exchange_with_remembered_workspace(EMPTY_PAGE, "ws-remembered", &["am", "list"]);
    assert_success(&output, &["am", "list"]);
    assert_eq!(
        request_line(&request),
        "GET /api/v3/workspaces/ws-remembered/analysis-models HTTP/1.1"
    );
}

#[test]
fn create_posts_the_documented_body() {
    let (request, output) = exchange(
        ONE_MODEL,
        &[
            "am",
            "create",
            "--workspace",
            "ws-1",
            "--name",
            "Sales",
            "--type",
            "ASK_DATA",
            "--datasource",
            "ds-1",
            "--description",
            "d",
            "--custom-id",
            "my-model",
            "--fork-from",
            "am-src",
        ],
    );
    let stdout = assert_success(&output, &["am", "create"]);
    assert_eq!(request_line(&request), format!("POST {BASE} HTTP/1.1"));
    assert_eq!(
        body_json(&request),
        serde_json::json!({
            "name": "Sales",
            "type": "ASK_DATA",
            "db_datasource_id": "ds-1",
            "description": "d",
            "custom_id": "my-model",
            "fork_from": "am-src"
        })
    );
    assert!(
        stdout.contains("\"future_field\": \"kept\""),
        "fields the CLI does not model must still be printed: {stdout}"
    );
}

#[test]
fn create_omits_optional_fields_that_were_not_passed() {
    let (request, _) = exchange(
        ONE_MODEL,
        &[
            "am",
            "create",
            "--workspace",
            "ws-1",
            "--name",
            "Sales",
            "--type",
            "ASK_DATA",
            "--datasource",
            "ds-1",
        ],
    );
    assert_eq!(
        body_json(&request),
        serde_json::json!({"name": "Sales", "type": "ASK_DATA", "db_datasource_id": "ds-1"})
    );
}

#[test]
fn get_by_custom_id_sets_the_flag() {
    let (request, output) = exchange(
        ONE_MODEL,
        &[
            "am",
            "get",
            "--workspace",
            "ws-1",
            "my-model",
            "--by-custom-id",
        ],
    );
    assert_success(&output, &["am", "get"]);
    assert_eq!(
        request_line(&request),
        format!("GET {BASE}/my-model?by_custom_id=true HTTP/1.1")
    );
}

#[test]
fn update_patches_only_the_given_fields() {
    let (request, output) = exchange(
        ONE_MODEL,
        &[
            "am",
            "update",
            "--workspace",
            "ws-1",
            "am-1",
            "--description",
            "",
        ],
    );
    assert_success(&output, &["am", "update"]);
    assert_eq!(
        request_line(&request),
        format!("PATCH {BASE}/am-1 HTTP/1.1")
    );
    assert_eq!(body_json(&request), serde_json::json!({"description": ""}));
}

#[test]
fn delete_accepts_an_empty_data_object() {
    // `ResponseWrapperVoid` documents `data` as an object; `{}` must not fail
    // decoding the way it would with a unit type.
    let (request, output) = exchange(EMPTY_DATA, &["am", "delete", "--workspace", "ws-1", "am-1"]);
    let stdout = assert_success(&output, &["am", "delete"]);
    assert_eq!(
        request_line(&request),
        format!("DELETE {BASE}/am-1 HTTP/1.1")
    );
    assert_eq!(
        stdout.trim(),
        "Deleted analysis model `am-1` in workspace `ws-1`"
    );
}

#[test]
fn entry_list_sends_kind_filters_and_repeated_ids() {
    let (request, output) = exchange(
        EMPTY_PAGE,
        &[
            "am",
            "entry",
            "list",
            "--workspace",
            "ws-1",
            "--model",
            "am-1",
            "--entity-type",
            "few_shot",
            "--keyword",
            "revenue",
            "--from",
            "MANUAL",
            "--ids",
            "e-1,e-2",
            "--page-size",
            "3",
        ],
    );
    assert_success(&output, &["am", "entry", "list"]);
    assert_eq!(
        request_line(&request),
        format!(
            "GET {BASE}/am-1/entries?entity_type=few_shot&keyword=revenue&from=MANUAL&ids=e-1&ids=e-2&page_size=3 HTTP/1.1"
        )
    );
}

#[test]
fn entry_get_passes_the_kind_as_a_query_parameter() {
    let (request, output) = exchange(
        ONE_ENTRY,
        &[
            "am",
            "entry",
            "get",
            "--workspace",
            "ws-1",
            "--model",
            "am-1",
            "e-1",
            "--entity-type",
            "few_shot",
        ],
    );
    let stdout = assert_success(&output, &["am", "entry", "get"]);
    assert_eq!(
        request_line(&request),
        format!("GET {BASE}/am-1/entries/e-1?entity_type=few_shot HTTP/1.1")
    );
    assert!(stdout.contains("\"owner\": \"team\""), "{stdout}");
}

#[test]
fn entry_create_posts_an_inline_payload() {
    let (request, output) = exchange(
        ONE_ENTRY,
        &[
            "am",
            "entry",
            "create",
            "--workspace",
            "ws-1",
            "--model",
            "am-1",
            "--entity-type",
            "few_shot",
            "--embedding",
            "monthly revenue",
            "--payload",
            r#"{"artifact":{"few_shot":{"type":"SQL","content":"SELECT 1"}}}"#,
            "--extra",
            r#"{"owner":"team"}"#,
            "--disabled",
        ],
    );
    assert_success(&output, &["am", "entry", "create"]);
    assert_eq!(
        request_line(&request),
        format!("POST {BASE}/am-1/entries HTTP/1.1")
    );
    assert_eq!(
        body_json(&request),
        serde_json::json!({
            "entity_type": "few_shot",
            "embedding": "monthly revenue",
            "payload": {"artifact": {"few_shot": {"type": "SQL", "content": "SELECT 1"}}},
            "extra": {"owner": "team"},
            "disabled": true
        })
    );
}

#[test]
fn entry_create_reads_the_payload_from_a_file() {
    let dir = std::env::temp_dir().join(unique_name("am-payload"));
    fs::create_dir_all(&dir).expect("create scratch dir");
    let path = dir.join("payload.json");
    fs::write(
        &path,
        r#"{"artifact":{"biz_rule":{"type":"TEXT","content":"rule"}}}"#,
    )
    .expect("write payload file");
    let path_arg = path.to_string_lossy().into_owned();

    let (request, output) = exchange(
        ONE_ENTRY,
        &[
            "am",
            "entry",
            "create",
            "--workspace",
            "ws-1",
            "--model",
            "am-1",
            "--entity-type",
            "biz_rule",
            "--payload-file",
            path_arg.as_str(),
        ],
    );
    let _ = fs::remove_dir_all(&dir);
    assert_success(&output, &["am", "entry", "create", "--payload-file"]);
    assert_eq!(
        body_json(&request),
        serde_json::json!({
            "entity_type": "biz_rule",
            "payload": {"artifact": {"biz_rule": {"type": "TEXT", "content": "rule"}}}
        }),
        "an omitted embedding and an unset --disabled must not be sent"
    );
}

#[test]
fn entry_update_patches_the_kind_and_the_given_fields() {
    let (request, output) = exchange(
        ONE_ENTRY,
        &[
            "am",
            "entry",
            "update",
            "--workspace",
            "ws-1",
            "--model",
            "am-1",
            "e-1",
            "--entity-type",
            "few_shot",
            "--extra",
            "{}",
        ],
    );
    assert_success(&output, &["am", "entry", "update"]);
    assert_eq!(
        request_line(&request),
        format!("PATCH {BASE}/am-1/entries/e-1 HTTP/1.1")
    );
    assert_eq!(
        body_json(&request),
        serde_json::json!({"entity_type": "few_shot", "extra": {}})
    );
}

#[test]
fn entry_delete_passes_the_kind_as_a_query_parameter() {
    let (request, output) = exchange(
        NO_DATA,
        &[
            "am",
            "entry",
            "delete",
            "--workspace",
            "ws-1",
            "--model",
            "am-1",
            "e-1",
            "--entity-type",
            "drilldown_entity",
        ],
    );
    let stdout = assert_success(&output, &["am", "entry", "delete"]);
    assert_eq!(
        request_line(&request),
        format!("DELETE {BASE}/am-1/entries/e-1?entity_type=drilldown_entity HTTP/1.1")
    );
    assert_eq!(
        stdout.trim(),
        "Deleted entry `e-1` from analysis model `am-1`"
    );
}

#[test]
fn entry_disable_and_enable_set_an_explicit_flag() {
    for (verb, disabled, printed) in [("disable", true, "Disabled"), ("enable", false, "Enabled")] {
        let (request, output) = exchange(
            EMPTY_DATA,
            &[
                "am",
                "entry",
                verb,
                "--workspace",
                "ws-1",
                "--model",
                "am-1",
                "e-1",
                "--entity-type",
                "few_shot",
            ],
        );
        let stdout = assert_success(&output, &["am", "entry", verb]);
        assert_eq!(
            request_line(&request),
            format!("POST {BASE}/am-1/entries/e-1/disable HTTP/1.1")
        );
        assert_eq!(
            body_json(&request),
            serde_json::json!({"disabled": disabled, "entity_type": "few_shot"})
        );
        assert_eq!(
            stdout.trim(),
            format!("{printed} entry `e-1` of analysis model `am-1`")
        );
    }
}
