//! Wire-level `datasource` tests against a loopback stub of the API.
//!
//! No datasource could be created on production (it needs a working
//! connection), so these are what pin the request shapes to the spec.

use serde_json::{Value, json};

use crate::common::assert_success;
use crate::common::stub::{exchange_with_remembered_workspace, request_body, request_line};

const WS: &str = "ws-remembered";
const DATASOURCE: &str = r#"{"success":true,"data":{"id":"ds-1","name":"sales","schemas":["public"],"db_connection_id":"conn-1","building_version":"20261009","build_version":""}}"#;
const VOID: &str = r#"{"success":true,"data":{}}"#;

fn body_json(request: &str) -> Value {
    serde_json::from_str(request_body(request)).expect("request body is JSON")
}

#[test]
fn list_uses_the_remembered_workspace_and_every_filter() {
    let args = [
        "datasource",
        "list",
        "--connection",
        "conn-1",
        "--name",
        "sal",
        "--page-size",
        "3",
        "--continuation-token",
        "tok",
    ];
    let (request, output) =
        exchange_with_remembered_workspace(r#"{"success":true,"data":{"items":[]}}"#, WS, &args);
    assert_success(&output, &args);
    assert_eq!(
        request_line(&request),
        "GET /api/v3/workspaces/ws-remembered/db-datasources?db_connection_id=conn-1&name_fuzzy=sal&page_size=3&continuation_token=tok HTTP/1.1"
    );
}

#[test]
fn create_sends_one_schema_as_a_list() {
    let args = [
        "datasource",
        "create",
        "--connection",
        "conn-1",
        "--schema",
        "public",
        "--name",
        "sales",
        "--description",
        "orders db",
        "--custom-id",
        "ds-ext-1",
        "--table-filter",
        "^orders",
    ];
    let (request, output) = exchange_with_remembered_workspace(DATASOURCE, WS, &args);
    let stdout = assert_success(&output, &args);
    assert_eq!(
        request_line(&request),
        "POST /api/v3/workspaces/ws-remembered/db-datasources HTTP/1.1"
    );
    assert_eq!(
        body_json(&request),
        json!({
            "schemas": ["public"],
            "name": "sales",
            "description": "orders db",
            "db_connection_id": "conn-1",
            "custom_id": "ds-ext-1",
            "table_filter_rule": "^orders"
        })
    );
    assert!(
        stdout.contains("\"building_version\": \"20261009\""),
        "{stdout}"
    );
}

#[test]
fn get_update_delete_build_address_one_datasource() {
    for (args, response, line, body) in [
        (
            vec!["datasource", "get", "ds-1"],
            DATASOURCE,
            "GET /api/v3/workspaces/ws-explicit/db-datasources/ds-1 HTTP/1.1",
            None,
        ),
        (
            vec!["datasource", "get", "ext-1", "--by-custom-id"],
            DATASOURCE,
            "GET /api/v3/workspaces/ws-explicit/db-datasources/ext-1?by_custom_id=true HTTP/1.1",
            None,
        ),
        (
            vec!["datasource", "update", "ds-1", "--table-filter", ".*"],
            DATASOURCE,
            "PATCH /api/v3/workspaces/ws-explicit/db-datasources/ds-1 HTTP/1.1",
            Some(json!({"table_filter_rule": ".*"})),
        ),
        (
            vec!["datasource", "delete", "ds-1"],
            VOID,
            "DELETE /api/v3/workspaces/ws-explicit/db-datasources/ds-1 HTTP/1.1",
            None,
        ),
        (
            vec!["datasource", "build", "ds-1"],
            VOID,
            "POST /api/v3/workspaces/ws-explicit/db-datasources/ds-1/build HTTP/1.1",
            Some(json!({})),
        ),
    ] {
        let mut args = args;
        args.extend(["--workspace", "ws-explicit"]);
        let (request, output) = exchange_with_remembered_workspace(response, WS, &args);
        assert_success(&output, &args);
        assert_eq!(request_line(&request), line, "{args:?}");
        if let Some(body) = body {
            assert_eq!(body_json(&request), body, "{args:?}");
        }
    }
}

#[test]
fn void_actions_print_one_line() {
    let args = ["datasource", "build", "ds-1"];
    let (_, output) = exchange_with_remembered_workspace(VOID, WS, &args);
    let stdout = assert_success(&output, &args);
    assert!(stdout.contains("Started an index build"), "{stdout}");
    assert!(stdout.contains("building_version"), "{stdout}");
    // The follow-up command names the workspace actually used.
    assert!(
        stdout.contains(&format!("datasource get ds-1 --workspace {WS}")),
        "{stdout}"
    );

    let args = ["datasource", "delete", "ds-1"];
    let (_, output) = exchange_with_remembered_workspace(r#"{"success":true}"#, WS, &args);
    let stdout = assert_success(&output, &args);
    assert!(stdout.contains("Deleted datasource `ds-1`"), "{stdout}");
}

#[test]
fn tables_and_columns_send_their_filters() {
    let args = ["datasource", "tables", "ds-1", "--name", "ord"];
    let (request, output) = exchange_with_remembered_workspace(
        r#"{"success":true,"data":{"items":[{"schema_name":"public","table_name":"orders","table_type":"table"}]}}"#,
        WS,
        &args,
    );
    let stdout = assert_success(&output, &args);
    assert_eq!(
        request_line(&request),
        "GET /api/v3/workspaces/ws-remembered/db-datasources/ds-1/tables?name_fuzzy=ord HTTP/1.1"
    );
    assert!(stdout.contains("orders"), "{stdout}");

    let args = ["datasource", "columns", "ds-1", "--table", "order items"];
    let (request, output) = exchange_with_remembered_workspace(
        r#"{"success":true,"data":{"items":[{"column_name":"status","data_type":"varchar","embedding_enabled":false}]}}"#,
        WS,
        &args,
    );
    let stdout = assert_success(&output, &args);
    assert_eq!(
        request_line(&request),
        "GET /api/v3/workspaces/ws-remembered/db-datasources/ds-1/columns?table_name=order+items HTTP/1.1"
    );
    assert!(stdout.contains("\"embedding_enabled\": false"), "{stdout}");
}

#[test]
fn annotate_flags_make_one_column_edit() {
    let args = [
        "datasource",
        "annotate",
        "ds-1",
        "--table",
        "orders",
        "--column",
        "status",
        "--comment",
        "",
        "--embedding",
        "true",
    ];
    let (request, output) = exchange_with_remembered_workspace(VOID, WS, &args);
    let stdout = assert_success(&output, &args);
    assert_eq!(
        request_line(&request),
        "PATCH /api/v3/workspaces/ws-remembered/db-datasources/ds-1/schema-metadata HTTP/1.1"
    );
    // An empty --comment is sent as "", which clears the annotation.
    assert_eq!(
        body_json(&request),
        json!({"items": [{"target": "column", "table_name": "orders", "column_name": "status",
                          "comment": "", "embedding_enabled": true}]})
    );
    assert!(
        stdout.contains("Applied 1 schema metadata edit"),
        "{stdout}"
    );
}

#[test]
fn annotate_sends_a_json_batch_unchanged() {
    let edits = r#"[{"target":"table","table_name":"orders","comment":"One row per order"},
                    {"target":"column","table_name":"orders","column_name":"id","schema_name":"public","embedding_enabled":false}]"#;
    let args = ["datasource", "annotate", "ds-1", "--edits", edits];
    let (request, output) = exchange_with_remembered_workspace(VOID, WS, &args);
    let stdout = assert_success(&output, &args);
    assert_eq!(
        body_json(&request),
        json!({"items": [
            {"target": "table", "table_name": "orders", "comment": "One row per order"},
            {"target": "column", "table_name": "orders", "column_name": "id",
             "schema_name": "public", "embedding_enabled": false}
        ]})
    );
    assert!(
        stdout.contains("Applied 2 schema metadata edit"),
        "{stdout}"
    );
}
