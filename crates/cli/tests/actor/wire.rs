//! Wire-level `actor` tests against a loopback stub of the MemoryLake API.
//!
//! These need no API key: they pin the HTTP method, path, query string, and
//! body that each subcommand sends, and the output it prints back. Live tests
//! prove the endpoints exist; these prove the CLI calls the documented ones.

use crate::common::assert_success;
use crate::common::stub::{exchange, request_line};

const EMPTY_PAGE: &str = r#"{"success":true,"data":{"items":[],"continuation_token":null}}"#;
const ONE_ACTOR: &str = r#"{"success":true,"data":{"id":"act-1","custom_id":"user-1","actor_type":"HUMAN","display_name":"Alice"}}"#;
const NO_DATA: &str = r#"{"success":true,"message":"Operation completed successfully"}"#;

#[test]
fn list_calls_the_account_wide_endpoint() {
    let (request, output) = exchange(
        EMPTY_PAGE,
        &[
            "actor",
            "list",
            "--page-size",
            "2",
            "--continuation-token",
            "tok-1",
            "--type",
            "HUMAN",
            "--name",
            "Ali",
        ],
    );
    assert_success(&output, &["actor", "list"]);

    let line = request_line(&request);
    assert!(line.starts_with("GET /api/v3/actors?"), "{line}");
    for expected in [
        "page_size=2",
        "continuation_token=tok-1",
        "actor_type=HUMAN",
        "display_name_fuzzy=Ali",
    ] {
        assert!(line.contains(expected), "missing {expected} in {line}");
    }
}

#[test]
fn list_repeats_the_tags_parameter_once_per_tag() {
    let (request, output) = exchange(EMPTY_PAGE, &["actor", "list", "--tags", "vip,cn"]);
    assert_success(&output, &["actor", "list", "--tags"]);

    let line = request_line(&request);
    assert!(line.contains("tags=vip"), "{line}");
    assert!(line.contains("tags=cn"), "{line}");
    assert!(
        !line.contains("tags=vip%2Ccn") && !line.contains("tags=vip,cn"),
        "tags must be sent as repeated parameters, not one joined value: {line}"
    );
}

#[test]
fn list_sends_no_tags_parameter_when_the_flag_is_absent() {
    let (request, _) = exchange(EMPTY_PAGE, &["actor", "list"]);
    assert!(!request_line(&request).contains("tags"), "{request}");
}

#[test]
fn workspace_scoped_list_filters_by_tag_too() {
    let (request, output) = exchange(
        EMPTY_PAGE,
        &["actor", "list", "--workspace", "ws-1", "--tags", "vip"],
    );
    assert_success(&output, &["actor", "list", "--workspace", "--tags"]);

    let line = request_line(&request);
    assert!(
        line.starts_with("GET /api/v3/workspaces/ws-1/actors?"),
        "{line}"
    );
    assert!(line.contains("tags=vip"), "{line}");
}

#[test]
fn list_with_workspace_calls_the_workspace_scoped_endpoint() {
    let (request, output) = exchange(
        EMPTY_PAGE,
        &["actor", "list", "--workspace", "ws-1", "--page-size", "2"],
    );
    assert_success(&output, &["actor", "list", "--workspace"]);

    let line = request_line(&request);
    assert!(
        line.starts_with("GET /api/v3/workspaces/ws-1/actors?"),
        "{line}"
    );
    assert!(line.contains("page_size=2"), "{line}");
}

#[test]
fn create_posts_the_documented_body() {
    let (request, output) = exchange(
        ONE_ACTOR,
        &[
            "actor",
            "create",
            "--custom-id",
            "user-1",
            "--display-name",
            "Alice",
            "--type",
            "ASSISTANT",
            "--description",
            "intake bot",
            "--metadata",
            r#"{"tier":"premium"}"#,
        ],
    );
    assert_success(&output, &["actor", "create"]);

    assert!(
        request_line(&request).starts_with("POST /api/v3/actors "),
        "{}",
        request_line(&request)
    );
    for expected in [
        r#""custom_id":"user-1""#,
        r#""display_name":"Alice""#,
        r#""actor_type":"ASSISTANT""#,
        r#""description":"intake bot""#,
        r#""metadata":{"tier":"premium"}"#,
    ] {
        assert!(
            request.contains(expected),
            "missing {expected} in {request}"
        );
    }
}

#[test]
fn create_omits_flags_that_were_not_passed() {
    let (request, _) = exchange(
        ONE_ACTOR,
        &[
            "actor",
            "create",
            "--custom-id",
            "user-1",
            "--display-name",
            "Alice",
        ],
    );
    assert!(!request.contains("actor_type"), "{request}");
    assert!(!request.contains("description"), "{request}");
    assert!(!request.contains("metadata"), "{request}");
    assert!(!request.contains("tags"), "{request}");
}

#[test]
fn create_sends_tags_as_a_json_array() {
    let (request, output) = exchange(
        ONE_ACTOR,
        &[
            "actor",
            "create",
            "--custom-id",
            "user-1",
            "--display-name",
            "Alice",
            "--tags",
            "vip, cn",
        ],
    );
    assert_success(&output, &["actor", "create", "--tags"]);
    assert!(
        request.contains(r#""tags":["vip","cn"]"#),
        "tags must be a JSON array of trimmed strings: {request}"
    );
}

#[test]
fn get_uses_the_id_path_and_by_custom_id_query() {
    let (request, output) = exchange(ONE_ACTOR, &["actor", "get", "act-1"]);
    assert_success(&output, &["actor", "get"]);
    assert_eq!(
        request_line(&request).split(' ').nth(1),
        Some("/api/v3/actors/act-1")
    );
    assert!(request_line(&request).starts_with("GET "));

    let (request, output) = exchange(ONE_ACTOR, &["actor", "get", "user-1", "--by-custom-id"]);
    assert_success(&output, &["actor", "get", "--by-custom-id"]);
    assert_eq!(
        request_line(&request).split(' ').nth(1),
        Some("/api/v3/actors/user-1?by_custom_id=true")
    );
}

#[test]
fn update_patches_only_the_fields_passed() {
    let (request, output) = exchange(
        ONE_ACTOR,
        &[
            "actor",
            "update",
            "act-1",
            "--description",
            "updated",
            "--metadata",
            r#"{"tier":"enterprise"}"#,
        ],
    );
    assert_success(&output, &["actor", "update"]);

    let line = request_line(&request);
    assert!(line.starts_with("PATCH /api/v3/actors/act-1 "), "{line}");
    assert!(
        request.contains(r#"{"description":"updated","metadata":{"tier":"enterprise"}}"#),
        "{request}"
    );
    assert!(
        !request.contains("display_name"),
        "an omitted field must not be sent: {request}"
    );
    assert!(
        !request.contains("tags"),
        "an omitted tag list must not be sent, or the server would replace the stored tags: {request}"
    );
}

#[test]
fn update_replaces_tags_with_the_list_passed() {
    let (request, output) = exchange(ONE_ACTOR, &["actor", "update", "act-1", "--tags", "gold"]);
    assert_success(&output, &["actor", "update", "--tags"]);

    let line = request_line(&request);
    assert!(line.starts_with("PATCH /api/v3/actors/act-1 "), "{line}");
    assert!(request.contains(r#"{"tags":["gold"]}"#), "{request}");
}

#[test]
fn clear_tags_sends_an_empty_array() {
    // Omitting `tags` means "leave them alone", so clearing has to be an
    // explicit empty list on the wire.
    let (request, output) = exchange(ONE_ACTOR, &["actor", "update", "act-1", "--clear-tags"]);
    assert_success(&output, &["actor", "update", "--clear-tags"]);
    assert!(request.contains(r#"{"tags":[]}"#), "{request}");
}

#[test]
fn tags_and_status_from_the_server_reach_the_output() {
    // Both fields were previously dropped on the floor: the CLI re-serializes
    // the decoded struct, so a field it does not know about never prints.
    let (_, output) = exchange(
        r#"{"success":true,"data":{"items":[{"actor_id":"act-1","display_name":"Ada","tags":["vip","cn"],"status":"ACTIVE","bound_at":"2026-08-19T09:14:37Z"}],"continuation_token":null}}"#,
        &["actor", "list", "--workspace", "ws-1"],
    );
    let stdout = assert_success(&output, &["actor", "list", "--workspace"]);
    assert!(stdout.contains("\"vip\""), "{stdout}");
    assert!(stdout.contains("\"cn\""), "{stdout}");
    assert!(stdout.contains("\"status\": \"ACTIVE\""), "{stdout}");
}

#[test]
fn actor_tags_from_the_server_reach_the_output() {
    let (_, output) = exchange(
        r#"{"success":true,"data":{"id":"act-1","actor_type":"HUMAN","display_name":"Ada","tags":["vip"]}}"#,
        &["actor", "get", "act-1"],
    );
    let stdout = assert_success(&output, &["actor", "get"]);
    assert!(stdout.contains("\"tags\""), "{stdout}");
    assert!(stdout.contains("\"vip\""), "{stdout}");
}

#[test]
fn delete_uses_delete_and_prints_a_one_line_confirmation() {
    let (request, output) = exchange(NO_DATA, &["actor", "delete", "act-1"]);
    let stdout = assert_success(&output, &["actor", "delete"]);

    let line = request_line(&request);
    assert!(line.starts_with("DELETE /api/v3/actors/act-1 "), "{line}");
    assert_eq!(stdout.trim(), "Deleted actor `act-1`");
}

#[test]
fn bind_posts_actor_id_to_the_workspace_endpoint() {
    let (request, output) = exchange(
        r#"{"success":true,"data":{"actor_id":"act-1","bound_at":"2025-03-15T09:00:00Z"}}"#,
        &["actor", "bind", "--workspace", "ws-1", "--actor", "act-1"],
    );
    let stdout = assert_success(&output, &["actor", "bind"]);

    let line = request_line(&request);
    assert!(
        line.starts_with("POST /api/v3/workspaces/ws-1/actors "),
        "{line}"
    );
    assert!(request.contains(r#"{"actor_id":"act-1"}"#), "{request}");
    assert!(
        stdout.contains("\"bound_at\": \"2025-03-15T09:00:00Z\""),
        "{stdout}"
    );
}

#[test]
fn unbind_deletes_the_binding_and_prints_a_one_line_confirmation() {
    let (request, output) = exchange(
        NO_DATA,
        &["actor", "unbind", "--workspace", "ws-1", "--actor", "act-1"],
    );
    let stdout = assert_success(&output, &["actor", "unbind"]);

    let line = request_line(&request);
    assert!(
        line.starts_with("DELETE /api/v3/workspaces/ws-1/actors/act-1 "),
        "{line}"
    );
    assert_eq!(stdout.trim(), "Unbound actor `act-1` from workspace `ws-1`");
}

#[test]
fn ids_are_encoded_into_their_own_path_segment() {
    let (request, _) = exchange(ONE_ACTOR, &["actor", "get", "weird id/here?x"]);
    assert_eq!(
        request_line(&request).split(' ').nth(1),
        Some("/api/v3/actors/weird%20id%2Fhere%3Fx")
    );
}

#[test]
fn unknown_actor_type_from_the_server_is_printed_not_rejected() {
    // A type added server-side must not break the command, and the raw value
    // must survive into the output.
    let (_, output) = exchange(
        r#"{"success":true,"data":{"id":"act-1","actor_type":"SUPERVISOR","display_name":"Ada"}}"#,
        &["actor", "get", "act-1"],
    );
    let stdout = assert_success(&output, &["actor", "get"]);
    assert!(
        stdout.contains("\"actor_type\": \"SUPERVISOR\""),
        "unknown actor_type must round-trip into the output: {stdout}"
    );
}

#[test]
fn server_errors_are_surfaced_verbatim() {
    let (_, output) = exchange(
        r#"{"success":false,"message":"custom_id already exists","error_code":"ACTOR_CUSTOM_ID_CONFLICT"}"#,
        &[
            "actor",
            "create",
            "--custom-id",
            "user-1",
            "--display-name",
            "Alice",
        ],
    );

    assert!(!output.status.success(), "duplicate create should fail");
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(stderr.contains("custom_id already exists"), "{stderr}");
    assert!(stderr.contains("[ACTOR_CUSTOM_ID_CONFLICT]"), "{stderr}");
}
