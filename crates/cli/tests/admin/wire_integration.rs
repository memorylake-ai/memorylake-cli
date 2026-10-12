//! Wire-level tests for `integration` (`im`).
//!
//! Besides the usual method/path/body pins, these prove the credentials go
//! where they must — into the request body — and nowhere else: not into the
//! output, whatever source they were read from.

use std::fs;

use crate::common::stub::{
    exchange, exchange_sequence, exchange_with_input, request_body, request_header, request_line,
};
use crate::common::{assert_failure, assert_success};

const EMPTY_DATA: &str = r#"{"success":true,"data":{}}"#;

/// An integration as the API returns it, in `status`.
fn integration(status: &str, error: &str) -> String {
    format!(
        r#"{{"success":true,"data":{{"id":"im-1","channel":"slack","name":"Help","agent_id":"ag-1","workspace_id":"ws-1","read_write_project_id":"p-1","read_only_project_ids":[],"dataset_ids":[],"external_app_id":"T1:U1","credentials_configured":true,"settings":{{}},"daily_message_limit":0,"enabled":true,"status":"{status}"{error},"created_at":1,"updated_at":2}}}}"#
    )
}

const SLACK_CREDS: &str = r#"{"botToken":"xoxb-wire-secret","appToken":"xapp-wire-secret"}"#;

fn create_args<'a>(extra: &[&'a str]) -> Vec<&'a str> {
    let mut args = vec![
        "im",
        "create",
        "--channel",
        "slack",
        "--workspace",
        "ws-1",
        "--name",
        "Help",
        "--agent",
        "ag-1",
        "--read-write-project",
        "p-1",
    ];
    args.extend_from_slice(extra);
    args
}

#[test]
fn list_filters_ride_the_query_string() {
    let page = r#"{"success":true,"data":{"items":[],"total":0}}"#;
    let args = [
        "integration",
        "list",
        "--workspace",
        "ws-1",
        "--agent",
        "ag-1",
        "--page-size",
        "5",
    ];
    let (request, output) = exchange(page, &args);
    assert_success(&output, &args);

    assert_eq!(
        request_line(&request),
        "GET /admin/v1/integrations?page_size=5&workspace_id=ws-1&agent_id=ag-1 HTTP/1.1"
    );
}

#[test]
fn get_reads_one_integration() {
    let (request, output) = exchange(&integration("connected", ""), &["im", "get", "im-1"]);
    assert_success(&output, &["im", "get", "im-1"]);
    assert_eq!(
        request_line(&request),
        "GET /admin/v1/integrations/im-1 HTTP/1.1"
    );
}

#[test]
fn create_sends_credentials_from_the_environment_in_the_body_only() {
    let args = create_args(&[
        "--credentials-env",
        "MLCLI_WIRE_SLACK",
        "--idempotency-key",
        "idem-im-1",
    ]);
    let (request, output) = exchange_with_input(
        &integration("pending", ""),
        &args,
        "",
        &[("MLCLI_WIRE_SLACK", SLACK_CREDS)],
    );
    let stdout = assert_success(&output, &args);

    assert_eq!(
        request_line(&request),
        "POST /admin/v1/integrations HTTP/1.1"
    );
    assert_eq!(
        request_body(&request),
        r#"{"channel":"slack","workspace_id":"ws-1","name":"Help","agent_id":"ag-1","read_write_project_id":"p-1","credentials":{"appToken":"xapp-wire-secret","botToken":"xoxb-wire-secret"}}"#
    );
    assert_eq!(
        request_header(&request, "idempotency-key"),
        Some("idem-im-1")
    );
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        !stdout.contains("wire-secret") && !stderr.contains("wire-secret"),
        "a credential leaked into the output:\n{stdout}\n{stderr}"
    );
}

#[test]
fn create_reads_credentials_from_stdin_with_every_option() {
    let args = create_args(&[
        "--credentials-stdin",
        "--read-only-project",
        "p-2,p-3",
        "--dataset",
        "ds-1",
        "--daily-message-limit",
        "500",
        "--card-template-id",
        "tpl-1",
    ]);
    let (request, output) = exchange_with_input(
        &integration("pending", ""),
        &args,
        &format!("{SLACK_CREDS}\n"),
        &[],
    );
    assert_success(&output, &args);

    assert_eq!(
        request_body(&request),
        r#"{"channel":"slack","workspace_id":"ws-1","name":"Help","agent_id":"ag-1","read_write_project_id":"p-1","read_only_project_ids":["p-2","p-3"],"dataset_ids":["ds-1"],"daily_message_limit":500,"settings":{"card_template_id":"tpl-1"},"credentials":{"appToken":"xapp-wire-secret","botToken":"xoxb-wire-secret"}}"#
    );
}

#[test]
fn create_wait_follows_the_status_until_it_settles() {
    let dir = std::env::temp_dir().join(format!("mlcli-im-wait-{}", std::process::id()));
    fs::create_dir_all(&dir).unwrap();
    let file = dir.join("creds.json");
    fs::write(&file, SLACK_CREDS).unwrap();
    let path = file.to_string_lossy().into_owned();
    let args = create_args(&["--credentials-file", path.as_str(), "--wait"]);

    let pending = integration("pending", "");
    let connected = integration("connected", "");
    let (requests, output) = exchange_sequence(&[&pending, &connected], &args);
    let stdout = assert_success(&output, &args);
    assert_eq!(
        request_line(&requests[0]),
        "POST /admin/v1/integrations HTTP/1.1"
    );
    assert_eq!(
        request_line(&requests[1]),
        "GET /admin/v1/integrations/im-1 HTTP/1.1"
    );
    assert!(stdout.contains(r#""status": "connected""#), "{stdout}");

    // A connection that ends in error fails the command, naming the code.
    let failed = integration(
        "error",
        r#","status_error":"凭据无效","status_error_code":"credentials_invalid""#,
    );
    let (_, output) = exchange_sequence(&[&pending, &failed], &args);
    let err = assert_failure(&output, &args);
    assert!(err.contains("credentials_invalid"), "{err}");
    let _ = fs::remove_dir_all(&dir);
}

#[test]
fn update_sends_only_what_changes_and_clears_with_empty_values() {
    let args = [
        "im",
        "update",
        "im-1",
        "--no-datasets",
        "--card-template-id",
        "",
        "--disable",
    ];
    let (request, output) = exchange(&integration("disabled", ""), &args);
    assert_success(&output, &args);

    assert_eq!(
        request_line(&request),
        "PATCH /admin/v1/integrations/im-1 HTTP/1.1"
    );
    assert_eq!(
        request_body(&request),
        r#"{"dataset_ids":[],"settings":{"card_template_id":""},"enabled":false}"#
    );
}

#[test]
fn update_rotates_a_secret_alone() {
    let args = [
        "im",
        "update",
        "im-1",
        "--credentials-env",
        "MLCLI_WIRE_ROTATE",
    ];
    let (request, output) = exchange_with_input(
        &integration("pending", ""),
        &args,
        "",
        &[("MLCLI_WIRE_ROTATE", r#"{"appSecret":"rotated-secret"}"#)],
    );
    assert_success(&output, &args);

    assert_eq!(
        request_body(&request),
        r#"{"credentials":{"appSecret":"rotated-secret"}}"#
    );
}

#[test]
fn delete_deletes_and_confirms() {
    let args = ["im", "delete", "im-1", "--idempotency-key", "idem-del"];
    let (request, output) = exchange(EMPTY_DATA, &args);
    let stdout = assert_success(&output, &args);

    assert_eq!(
        request_line(&request),
        "DELETE /admin/v1/integrations/im-1 HTTP/1.1"
    );
    assert_eq!(
        request_header(&request, "idempotency-key"),
        Some("idem-del")
    );
    assert!(stdout.contains("deleted"), "{stdout}");
}
