//! Wire-level `db-connection` tests against a loopback stub of the API.
//!
//! Production could not create a connection while this was written (HTTP 500
//! for every create, measured 2026-10-09), so the request shapes here are what
//! pins the CLI to the published spec.

use std::fs;

use serde_json::{Value, json};

use crate::common::stub::{exchange, exchange_with_input, request_body, request_line};
use crate::common::{assert_failure, assert_success, unique_name};

const CONNECTION: &str = r#"{"success":true,"data":{"id":"conn-1","name":"analytics","host":"db.example.com","port":5432,"username":"ro","database":"analytics","datasource_count":0}}"#;
const VOID: &str = r#"{"success":true}"#;

fn body_json(request: &str) -> Value {
    serde_json::from_str(request_body(request)).expect("request body is JSON")
}

#[test]
fn list_sends_every_filter() {
    let args = [
        "db-connection",
        "list",
        "--name",
        "ana",
        "--page-size",
        "5",
        "--continuation-token",
        "tok",
    ];
    let (request, output) = exchange(r#"{"success":true,"data":{"items":[]}}"#, &args);
    let stdout = assert_success(&output, &args);
    assert_eq!(
        request_line(&request),
        "GET /api/v3/db-connections?page_size=5&continuation_token=tok&name_fuzzy=ana HTTP/1.1"
    );
    let printed: Value = serde_json::from_str(&stdout).expect("JSON output");
    assert_eq!(printed, json!({"items": []}));
}

#[test]
fn create_reads_the_password_from_a_file() {
    let dir = std::env::temp_dir().join(unique_name("dbconn-pw"));
    fs::create_dir_all(&dir).unwrap();
    let file = dir.join("pw");
    fs::write(&file, "s3cret pass\n").unwrap();

    let args = [
        "db-connection",
        "create",
        "--name",
        "analytics",
        "--host",
        "db.example.com",
        "--port",
        "6543",
        "--username",
        "ro",
        "--database",
        "analytics",
        "--description",
        "replica",
        "--custom-id",
        "conn-ext-1",
        "--password-file",
        file.to_str().unwrap(),
    ];
    let (request, output) = exchange(CONNECTION, &args);
    let stdout = assert_success(&output, &args);
    let _ = fs::remove_dir_all(&dir);

    assert_eq!(
        request_line(&request),
        "POST /api/v3/db-connections HTTP/1.1"
    );
    assert_eq!(
        body_json(&request),
        json!({
            "name": "analytics",
            "description": "replica",
            "host": "db.example.com",
            "port": 6543,
            "username": "ro",
            "password": "s3cret pass",
            "database": "analytics",
            "custom_id": "conn-ext-1"
        })
    );
    assert!(stdout.contains("\"datasource_count\": 0"), "{stdout}");
}

#[test]
fn create_reads_the_password_from_stdin() {
    let args = [
        "db-connection",
        "create",
        "--name",
        "a",
        "--host",
        "h",
        "--username",
        "u",
        "--database",
        "d",
        "--password-stdin",
    ];
    let (request, output) = exchange_with_input(CONNECTION, &args, "from-stdin\r\n", &[]);
    assert_success(&output, &args);
    let body = body_json(&request);
    assert_eq!(body["password"], "from-stdin");
    // Port left to the server's default.
    assert!(body.get("port").is_none(), "{body}");
}

#[test]
fn test_reads_the_password_from_an_environment_variable() {
    let args = [
        "db-connection",
        "test",
        "--host",
        "db.example.com",
        "--username",
        "ro",
        "--database",
        "analytics",
        "--password-env",
        "MLCLI_TEST_DB_PASSWORD",
    ];
    let (request, output) = exchange_with_input(
        r#"{"success":true,"data":{"schemas":["public","sales"]}}"#,
        &args,
        "",
        &[("MLCLI_TEST_DB_PASSWORD", "env-secret")],
    );
    let stdout = assert_success(&output, &args);
    assert_eq!(
        request_line(&request),
        "POST /api/v3/db-connections/connectivity-test HTTP/1.1"
    );
    assert_eq!(
        body_json(&request),
        json!({"host": "db.example.com", "username": "ro", "password": "env-secret",
               "database": "analytics"})
    );
    let printed: Value = serde_json::from_str(&stdout).unwrap();
    assert_eq!(printed["schemas"], json!(["public", "sales"]));
}

#[test]
fn a_trace_never_shows_the_password() {
    let args = [
        "-vvv",
        "db-connection",
        "test",
        "--host",
        "h",
        "--username",
        "u",
        "--database",
        "d",
        "--password-stdin",
    ];
    let (request, output) = exchange_with_input(
        r#"{"success":true,"data":{"schemas":[]}}"#,
        &args,
        "trace-secret-9z",
        &[],
    );
    assert_success(&output, &args);
    // It did go out on the wire...
    assert_eq!(body_json(&request)["password"], "trace-secret-9z");
    // ...but not into the log.
    // The log goes to stdout alongside the result, so check both streams.
    let log = format!(
        "{}{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    assert!(log.contains("HTTP request"), "no trace: {log}");
    assert!(log.contains("REDACTED"), "{log}");
    assert!(!log.contains("trace-secret-9z"), "{log}");
}

#[test]
fn an_internal_error_on_create_points_at_the_connectivity_test() {
    let args = [
        "db-connection",
        "create",
        "--name",
        "a",
        "--host",
        "h",
        "--username",
        "u",
        "--database",
        "d",
        "--password-stdin",
    ];
    // What production answered on 2026-10-09.
    let (_, output) = exchange_with_input(
        r#"{"success":false,"message":"An unexpected error occurred. Please try again later.","error_code":"INTERNAL_ERROR"}"#,
        &args,
        "p",
        &[],
    );
    let output = assert_failure(&output, &args);
    assert!(output.contains("INTERNAL_ERROR"), "{output}");
    assert!(output.contains("db-connection test"), "{output}");
}

#[test]
fn get_addresses_by_id_or_custom_id() {
    let args = ["db-connection", "get", "conn-1"];
    let (request, output) = exchange(CONNECTION, &args);
    assert_success(&output, &args);
    assert_eq!(
        request_line(&request),
        "GET /api/v3/db-connections/conn-1 HTTP/1.1"
    );

    let args = ["db-connection", "get", "ext-1", "--by-custom-id"];
    let (request, output) = exchange(CONNECTION, &args);
    assert_success(&output, &args);
    assert_eq!(
        request_line(&request),
        "GET /api/v3/db-connections/ext-1?by_custom_id=true HTTP/1.1"
    );
}

#[test]
fn update_sends_only_the_changed_fields() {
    let args = ["db-connection", "update", "conn-1", "--name", "renamed"];
    let (request, output) = exchange(CONNECTION, &args);
    assert_success(&output, &args);
    assert_eq!(
        request_line(&request),
        "PATCH /api/v3/db-connections/conn-1 HTTP/1.1"
    );
    assert_eq!(body_json(&request), json!({"name": "renamed"}));
}

#[test]
fn a_credential_update_carries_the_password() {
    let args = [
        "db-connection",
        "update",
        "conn-1",
        "--host",
        "new.example.com",
        "--port",
        "5433",
        "--password-stdin",
    ];
    let (request, output) = exchange_with_input(CONNECTION, &args, "rotated\n", &[]);
    assert_success(&output, &args);
    assert_eq!(
        body_json(&request),
        json!({"host": "new.example.com", "port": 5433, "password": "rotated"})
    );
}

#[test]
fn delete_accepts_an_empty_envelope() {
    let args = ["db-connection", "delete", "conn-1"];
    let (request, output) = exchange(VOID, &args);
    let stdout = assert_success(&output, &args);
    assert_eq!(
        request_line(&request),
        "DELETE /api/v3/db-connections/conn-1 HTTP/1.1"
    );
    assert!(
        stdout.contains("Deleted database connection `conn-1`"),
        "{stdout}"
    );
}

#[test]
fn schemas_reads_a_saved_connection() {
    let args = ["dbconn", "schemas", "conn-1"];
    let (request, output) = exchange(r#"{"success":true,"data":{"schemas":["public"]}}"#, &args);
    let stdout = assert_success(&output, &args);
    assert_eq!(
        request_line(&request),
        "GET /api/v3/db-connections/conn-1/schemas HTTP/1.1"
    );
    assert!(stdout.contains("public"), "{stdout}");
}
