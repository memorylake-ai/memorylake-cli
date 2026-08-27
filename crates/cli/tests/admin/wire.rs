//! Wire-level tests for the team-management commands.
//!
//! The management endpoints live under `/admin/v1` on the SAME base URL and
//! key as everything else — that is the whole design. These tests pin the
//! method, path, query, body, and `Idempotency-Key` header each subcommand
//! sends, and what it prints back.

use crate::common::assert_success;
use crate::common::stub::{exchange, request_line};

const EMPTY_PAGE: &str = r#"{"success":true,"message":"Operation completed successfully","data":{"items":[],"total":0}}"#;
const EMPTY_DATA: &str =
    r#"{"success":true,"message":"Operation completed successfully","data":{}}"#;

#[test]
fn team_get_reads_the_team_endpoint() {
    let team = r#"{"success":true,"data":{"id":"t-1","name":"Acme","type":"org","owner_principal_id":"prin-1","created_at":1756000000,"caller_role":"tenant_owner"}}"#;
    let (request, output) = exchange(team, &["team", "get"]);
    let stdout = assert_success(&output, &["team", "get"]);

    assert_eq!(request_line(&request), "GET /admin/v1/team HTTP/1.1");
    assert!(
        stdout.contains("\"tenant_owner\""),
        "prints the payload: {stdout}"
    );
}

#[test]
fn team_rename_patches_the_name_and_carries_the_idempotency_key() {
    let team = r#"{"success":true,"data":{"id":"t-1","name":"Renamed","type":"org","owner_principal_id":"prin-1","created_at":1756000000}}"#;
    let args = [
        "team",
        "rename",
        "--name",
        "Renamed",
        "--idempotency-key",
        "idem-team-1",
    ];
    let (request, output) = exchange(team, &args);
    assert_success(&output, &args);

    assert_eq!(request_line(&request), "PATCH /admin/v1/team HTTP/1.1");
    assert!(
        request.contains(r#"{"name":"Renamed"}"#),
        "body not sent: {request}"
    );
    assert!(
        request
            .to_ascii_lowercase()
            .contains("idempotency-key: idem-team-1"),
        "idempotency key not sent: {request}"
    );
}

#[test]
fn api_key_list_filters_ride_the_query_string() {
    let args = [
        "api-key",
        "list",
        "--page-size",
        "5",
        "--continuation-token",
        "tok",
        "--name",
        "ci",
    ];
    let (request, output) = exchange(EMPTY_PAGE, &args);
    assert_success(&output, &args);

    assert_eq!(
        request_line(&request),
        "GET /admin/v1/api-keys?page_size=5&continuation_token=tok&name_fuzzy=ci HTTP/1.1"
    );
}

#[test]
fn api_key_create_posts_the_request_and_prints_the_one_time_key() {
    let created = r#"{"success":true,"data":{"id":"42","name":"ci","key_prefix":"sk-abc12","key":"sk-abc1234567890"}}"#;
    let args = [
        "api-key",
        "create",
        "--name",
        "ci",
        "--member",
        "prin-bot",
        "--expires-at",
        "1790000000",
    ];
    let (request, output) = exchange(created, &args);
    let stdout = assert_success(&output, &args);

    assert_eq!(request_line(&request), "POST /admin/v1/api-keys HTTP/1.1");
    assert!(
        request
            .contains(r#"{"name":"ci","member_principal_id":"prin-bot","expires_at":1790000000}"#),
        "body not sent: {request}"
    );
    assert!(
        stdout.contains("sk-abc1234567890"),
        "the one-time key must be shown: {stdout}"
    );
}

#[test]
fn api_key_rotate_posts_to_the_rotate_endpoint() {
    let created = r#"{"success":true,"data":{"id":"42","name":"ci","key_prefix":"sk-new12","key":"sk-new1234567890"}}"#;
    let args = ["api-key", "rotate", "42", "--idempotency-key", "idem-rot-1"];
    let (request, output) = exchange(created, &args);
    let stdout = assert_success(&output, &args);

    assert_eq!(
        request_line(&request),
        "POST /admin/v1/api-keys/42/rotate HTTP/1.1"
    );
    assert!(
        request
            .to_ascii_lowercase()
            .contains("idempotency-key: idem-rot-1"),
        "idempotency key not sent: {request}"
    );
    assert!(stdout.contains("sk-new1234567890"), "{stdout}");
}

#[test]
fn api_key_revoke_deletes_and_confirms() {
    let args = ["api-key", "revoke", "42"];
    let (request, output) = exchange(EMPTY_DATA, &args);
    let stdout = assert_success(&output, &args);

    assert_eq!(
        request_line(&request),
        "DELETE /admin/v1/api-keys/42 HTTP/1.1"
    );
    assert!(stdout.contains("revoked"), "{stdout}");
}

#[test]
fn the_key_alias_reaches_the_same_endpoint() {
    let (request, output) = exchange(EMPTY_PAGE, &["key", "list"]);
    assert_success(&output, &["key", "list"]);
    assert_eq!(request_line(&request), "GET /admin/v1/api-keys HTTP/1.1");
}

#[test]
fn member_list_reads_the_roster() {
    let (request, output) = exchange(EMPTY_PAGE, &["member", "list"]);
    assert_success(&output, &["member", "list"]);
    assert_eq!(request_line(&request), "GET /admin/v1/members HTTP/1.1");
}

#[test]
fn member_create_posts_a_virtual_member() {
    let member = r#"{"success":true,"data":{"principal_id":"prin-bot","display_name":"CI Bot","member_type":"virtual","role":"tenant_member","joined_at":1756000000,"status":"active","used_tokens":0}}"#;
    let args = [
        "member",
        "create",
        "--name",
        "CI Bot",
        "--role",
        "tenant_member",
    ];
    let (request, output) = exchange(member, &args);
    let stdout = assert_success(&output, &args);

    assert_eq!(request_line(&request), "POST /admin/v1/members HTTP/1.1");
    assert!(
        request.contains(r#"{"display_name":"CI Bot","role":"tenant_member"}"#),
        "body not sent: {request}"
    );
    assert!(stdout.contains("prin-bot"), "{stdout}");
}

#[test]
fn member_set_role_patches_the_member() {
    let args = ["member", "set-role", "prin-1", "--role", "tenant_admin"];
    let (request, output) = exchange(EMPTY_DATA, &args);
    let stdout = assert_success(&output, &args);

    assert_eq!(
        request_line(&request),
        "PATCH /admin/v1/members/prin-1 HTTP/1.1"
    );
    assert!(
        request.contains(r#"{"role":"tenant_admin"}"#),
        "body not sent: {request}"
    );
    assert!(stdout.contains("tenant_admin"), "{stdout}");
}

#[test]
fn member_remove_deletes_the_member() {
    let args = ["member", "remove", "prin-1"];
    let (request, output) = exchange(EMPTY_DATA, &args);
    let stdout = assert_success(&output, &args);

    assert_eq!(
        request_line(&request),
        "DELETE /admin/v1/members/prin-1 HTTP/1.1"
    );
    assert!(stdout.contains("removed"), "{stdout}");
}

#[test]
fn invitation_create_posts_email_and_role() {
    let invitation = r#"{"success":true,"data":{"id":"7","email":"a@b.co","role":"tenant_member","status":"pending","created_at":1756000000,"expires_at":1756604800}}"#;
    let args = [
        "invitation",
        "create",
        "--email",
        "a@b.co",
        "--role",
        "tenant_member",
    ];
    let (request, output) = exchange(invitation, &args);
    assert_success(&output, &args);

    assert_eq!(
        request_line(&request),
        "POST /admin/v1/invitations HTTP/1.1"
    );
    assert!(
        request.contains(r#"{"email":"a@b.co","role":"tenant_member"}"#),
        "body not sent: {request}"
    );
}

#[test]
fn invitation_list_filters_by_status() {
    let args = ["invitation", "list", "--status", "pending"];
    let (request, output) = exchange(EMPTY_PAGE, &args);
    assert_success(&output, &args);

    assert_eq!(
        request_line(&request),
        "GET /admin/v1/invitations?status=pending HTTP/1.1"
    );
}

#[test]
fn invitation_revoke_deletes_via_the_invite_alias() {
    let args = ["invite", "revoke", "7"];
    let (request, output) = exchange(EMPTY_DATA, &args);
    let stdout = assert_success(&output, &args);

    assert_eq!(
        request_line(&request),
        "DELETE /admin/v1/invitations/7 HTTP/1.1"
    );
    assert!(stdout.contains("revoked"), "{stdout}");
}

#[test]
fn usage_sends_the_period_bounds() {
    let usage = r#"{"success":true,"data":{"period":{"start_date":"2026-08-01","end_date":"2026-08-27"},"quota":{"available_tokens":1000,"unlimited":false},"totals":{"requests":1,"prompt_tokens":2,"completion_tokens":3},"by_model":[]}}"#;
    let args = [
        "usage",
        "--start-date",
        "2026-08-01",
        "--end-date",
        "2026-08-27",
    ];
    let (request, output) = exchange(usage, &args);
    let stdout = assert_success(&output, &args);

    assert_eq!(
        request_line(&request),
        "GET /admin/v1/usage?start_date=2026-08-01&end_date=2026-08-27 HTTP/1.1"
    );
    assert!(stdout.contains("available_tokens"), "{stdout}");
}
