//! Wire-level tests for the team-management commands.
//!
//! The management endpoints live under `/admin/v1` on the SAME base URL and
//! key as everything else — that is the whole design. These tests pin the
//! method, path, query, body, and `Idempotency-Key` header each subcommand
//! sends, and what it prints back.

use crate::common::stub::{exchange, exchange_sequence, request_body, request_line};
use crate::common::{assert_failure, assert_success};

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
fn role_list_reads_the_catalog() {
    let roles = r#"{"success":true,"data":{"roles":[{"key":"tenant_owner","label":"Owner","built_in":true,"assignable":false,"admin_grant_only":false},{"key":"tenant_custom_a1","label":"Ops","description":"billing","built_in":false,"assignable":true,"admin_grant_only":false,"parent_role_key":"tenant_member"}]}}"#;
    let (request, output) = exchange(roles, &["role", "list"]);
    let stdout = assert_success(&output, &["role", "list"]);

    assert_eq!(request_line(&request), "GET /admin/v1/roles HTTP/1.1");
    assert!(
        stdout.contains("tenant_custom_a1") && stdout.contains("admin_grant_only"),
        "catalog not printed: {stdout}"
    );
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
    let usage = USAGE;
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

/// A usage report as production answers it (measured 2026-10-12).
const USAGE: &str = r#"{"success":true,"data":{"scope":"team","period":{"start_date":"2026-08-01","end_date":"2026-08-27","timezone":"Asia/Shanghai"},"quota":{"available_tokens":1000,"unlimited":false},"totals":{"requests":1,"prompt_tokens":2,"completion_tokens":3,"used_tokens":4},"by_model":[{"model":"gpt-5.4","requests":1,"prompt_tokens":2,"completion_tokens":3,"used_tokens":4}]}}"#;

/// An org team as `team get` reads it.
const ORG_TEAM: &str = r#"{"success":true,"data":{"id":"t-1","name":"Acme","type":"org","owner_principal_id":"prin-1","created_at":1756000000,"caller_role":"tenant_owner"}}"#;

/// One member as the roster reports it.
const MEMBER: &str = r#"{"success":true,"data":{"principal_id":"prin-b","display_name":"CI Bot","member_type":"virtual","role":"tenant_member","joined_at":1756000000,"status":"active","used_tokens":0,"max_tokens":5000}}"#;

#[test]
fn usage_keeps_the_fields_production_returns() {
    // `scope`, `timezone` and `used_tokens` were once dropped by the typed
    // decoder; they are what tells team usage from one's own, and the unit
    // the balance is charged in.
    let args = ["usage"];
    let (_, output) = exchange(USAGE, &args);
    let stdout = assert_success(&output, &args);
    for field in [
        r#""scope": "team""#,
        r#""timezone": "Asia/Shanghai""#,
        r#""used_tokens": 4"#,
    ] {
        assert!(stdout.contains(field), "missing {field}: {stdout}");
    }
    assert!(
        !stdout.contains("buckets") && !stdout.contains("by_member"),
        "breakdowns that were not asked for are not printed as null: {stdout}"
    );
}

#[test]
fn usage_daily_and_group_by_repeat_the_parameter() {
    let report = r#"{"success":true,"data":{"scope":"team","period":{"start_date":"2026-10-11","end_date":"2026-10-11","timezone":"Asia/Shanghai"},"quota":{"available_tokens":1,"unlimited":false},"totals":{"requests":1,"prompt_tokens":1,"completion_tokens":0,"used_tokens":1},"by_model":[],"by_member":[{"principal_id":"prin-1","display_name":"A","requests":1,"prompt_tokens":1,"completion_tokens":0,"used_tokens":1}],"buckets":[{"date":"2026-10-11","start_time":1791648000,"end_time":1791734400,"requests":1,"prompt_tokens":1,"completion_tokens":0,"used_tokens":1,"by_member":[{"principal_id":"prin-1","display_name":"A","requests":1,"prompt_tokens":1,"completion_tokens":0,"used_tokens":1}]}]}}"#;
    let args = ["usage", "--daily", "--group-by", "member,model"];
    let (request, output) = exchange(report, &args);
    let stdout = assert_success(&output, &args);

    assert_eq!(
        request_line(&request),
        "GET /admin/v1/usage?bucket_width=1d&group_by=member&group_by=model HTTP/1.1"
    );
    assert!(stdout.contains(r#""date": "2026-10-11""#), "{stdout}");
    assert!(stdout.contains(r#""by_member""#), "{stdout}");
}

#[test]
fn usage_summary_reads_the_summary_endpoint() {
    let window = r#"{"start_date":"2026-10-12","end_date":"2026-10-12","requests":1,"prompt_tokens":2,"completion_tokens":3,"used_tokens":4}"#;
    let summary = format!(
        r#"{{"success":true,"data":{{"scope":"self","timezone":"Asia/Shanghai","today":{window},"last_7_days":{window},"month_to_date":{window}}}}}"#
    );
    let args = ["usage", "summary"];
    let (request, output) = exchange(&summary, &args);
    let stdout = assert_success(&output, &args);

    assert_eq!(
        request_line(&request),
        "GET /admin/v1/usage/summary HTTP/1.1"
    );
    assert!(stdout.contains(r#""month_to_date""#), "{stdout}");
}

#[test]
fn usage_records_filters_ride_the_query_string() {
    let records = r#"{"success":true,"data":{"items":[{"id":"962908","created_at":1791798940,"kind":"model_call","model":"gpt-5.4","member":{"principal_id":"prin-1","display_name":"A"},"prompt_tokens":117563,"completion_tokens":8,"used_tokens":9578}],"continuation_token":"tok-2"}}"#;
    let args = [
        "usage",
        "records",
        "--start-time",
        "1791000000",
        "--end-time",
        "1791800000",
        "--model",
        "gpt-5.4,files_process",
        "--kind",
        "model_call",
        "--member",
        "prin-1",
        "--member",
        "prin-2",
        "--request-id",
        "req-9",
        "--page-size",
        "1",
    ];
    let (request, output) = exchange(records, &args);
    let stdout = assert_success(&output, &args);

    assert_eq!(
        request_line(&request),
        "GET /admin/v1/usage/records?page_size=1&start_time=1791000000&end_time=1791800000\
         &model=gpt-5.4&model=files_process&kind=model_call&principal_id=prin-1\
         &principal_id=prin-2&request_id=req-9 HTTP/1.1"
    );
    assert!(stdout.contains(r#""used_tokens": 9578"#), "{stdout}");
    assert!(
        stdout.contains(r#""continuation_token": "tok-2""#),
        "{stdout}"
    );
}

#[test]
fn api_key_list_filters_by_status_and_owner() {
    let args = [
        "api-key", "list", "--status", "disabled", "--owner", "prin-1",
    ];
    let (request, output) = exchange(EMPTY_PAGE, &args);
    assert_success(&output, &args);

    assert_eq!(
        request_line(&request),
        "GET /admin/v1/api-keys?status=disabled&owner_principal_id=prin-1 HTTP/1.1"
    );
}

#[test]
fn api_key_get_keeps_the_fields_production_returns() {
    // Measured 2026-10-12: every key now carries its suffix, owner, IP
    // restriction and system flag.
    let key = r#"{"success":true,"data":{"id":"12756","name":"ci","key_prefix":"10ff777d","key_suffix":"c26c","status":"enabled","created_at":1789616227,"allowed_ips":["10.0.0.0/8"],"owner":{"principal_id":"prin-1","display_name":"A","member_type":"human"},"is_system":false,"group":"g-1"}}"#;
    let args = ["api-key", "get", "12756"];
    let (request, output) = exchange(key, &args);
    let stdout = assert_success(&output, &args);

    assert_eq!(
        request_line(&request),
        "GET /admin/v1/api-keys/12756 HTTP/1.1"
    );
    for field in [
        r#""key_suffix": "c26c""#,
        r#""member_type": "human""#,
        r#""10.0.0.0/8""#,
        r#""is_system": false"#,
        r#""group": "g-1""#,
    ] {
        assert!(stdout.contains(field), "missing {field}: {stdout}");
    }
}

#[test]
fn api_key_create_sends_ip_restriction_and_routing_groups() {
    let created = r#"{"success":true,"data":{"id":"42","name":"ci","key_prefix":"abc12345","key_suffix":"wxyz","key":"sk-abc"}}"#;
    let args = [
        "api-key",
        "create",
        "--name",
        "ci",
        "--allowed-ip",
        "10.0.0.1,192.168.0.0/16",
        "--group",
        "g-1",
        "--backup-group",
        "g-2",
    ];
    let (request, output) = exchange(created, &args);
    assert_success(&output, &args);

    assert_eq!(
        request_body(&request),
        r#"{"name":"ci","allowed_ips":["10.0.0.1","192.168.0.0/16"],"group":"g-1","backup_group":"g-2"}"#
    );
}

#[test]
fn api_key_update_patches_only_the_given_fields() {
    let key = r#"{"success":true,"data":{"id":"42","name":"renamed","key_prefix":"abc12345","status":"disabled","created_at":1,"allowed_ips":[],"is_system":false}}"#;
    let args = [
        "api-key",
        "update",
        "42",
        "--name",
        "renamed",
        "--status",
        "disabled",
        "--expires-at",
        "0",
        "--no-ip-restriction",
        "--group",
        "",
    ];
    let (request, output) = exchange(key, &args);
    assert_success(&output, &args);

    assert_eq!(
        request_line(&request),
        "PATCH /admin/v1/api-keys/42 HTTP/1.1"
    );
    assert_eq!(
        request_body(&request),
        r#"{"name":"renamed","status":"disabled","expires_at":0,"allowed_ips":[],"group":""}"#
    );
}

#[test]
fn member_list_filters_ride_the_query_string() {
    let args = [
        "member",
        "list",
        "--role",
        "tenant_admin",
        "--email",
        "a@x.io,b@x.io",
        "--type",
        "human",
    ];
    let (request, output) = exchange(EMPTY_PAGE, &args);
    assert_success(&output, &args);

    assert_eq!(
        request_line(&request),
        "GET /admin/v1/members?role=tenant_admin&email=a%40x.io&email=b%40x.io&member_type=human HTTP/1.1"
    );
}

#[test]
fn member_get_reads_one_member() {
    let args = ["member", "get", "prin-b"];
    let (request, output) = exchange(MEMBER, &args);
    let stdout = assert_success(&output, &args);

    assert_eq!(
        request_line(&request),
        "GET /admin/v1/members/prin-b HTTP/1.1"
    );
    assert!(stdout.contains(r#""max_tokens": 5000"#), "{stdout}");
}

#[test]
fn member_update_patches_then_prints_the_member_as_it_now_stands() {
    let args = [
        "member",
        "update",
        "prin-b",
        "--max-tokens",
        "5000",
        "--idempotency-key",
        "idem-m",
    ];
    let (requests, output) = exchange_sequence(&[EMPTY_DATA, MEMBER], &args);
    let stdout = assert_success(&output, &args);

    assert_eq!(
        request_line(&requests[0]),
        "PATCH /admin/v1/members/prin-b HTTP/1.1"
    );
    assert_eq!(request_body(&requests[0]), r#"{"max_tokens":5000}"#);
    assert!(
        requests[0]
            .to_ascii_lowercase()
            .contains("idempotency-key: idem-m"),
        "{}",
        requests[0]
    );
    assert_eq!(
        request_line(&requests[1]),
        "GET /admin/v1/members/prin-b HTTP/1.1"
    );
    assert!(stdout.contains(r#""max_tokens": 5000"#), "{stdout}");
}

#[test]
fn invitation_get_reads_one_invitation() {
    let invitation = r#"{"success":true,"data":{"id":"7","email":"a@x.io","role":"tenant_member","status":"expired","created_at":1,"expires_at":2}}"#;
    let args = ["invitation", "get", "7"];
    let (request, output) = exchange(invitation, &args);
    let stdout = assert_success(&output, &args);

    assert_eq!(
        request_line(&request),
        "GET /admin/v1/invitations/7 HTTP/1.1"
    );
    assert!(stdout.contains(r#""expired""#), "{stdout}");
}

#[test]
fn invitation_list_repeats_the_email_filter() {
    let args = ["invite", "list", "--email", "a@x.io", "--email", "b@x.io"];
    let (request, output) = exchange(EMPTY_PAGE, &args);
    assert_success(&output, &args);

    assert_eq!(
        request_line(&request),
        "GET /admin/v1/invitations?email=a%40x.io&email=b%40x.io HTTP/1.1"
    );
}

#[test]
fn role_list_keeps_the_base_role() {
    let roles = r#"{"success":true,"data":{"roles":[{"key":"auditor","label":"Auditor","built_in":false,"assignable":true,"admin_grant_only":false,"base_role_key":"tenant_member","parent_role_key":"tenant_member"}]}}"#;
    let args = ["role", "list"];
    let (_, output) = exchange(roles, &args);
    let stdout = assert_success(&output, &args);

    assert!(
        stdout.contains(r#""base_role_key": "tenant_member""#),
        "{stdout}"
    );
}

#[test]
fn team_create_posts_the_name_and_prints_the_one_time_key() {
    let created = r#"{"success":true,"data":{"team":{"id":"t-9","name":"Lab","type":"org","owner_principal_id":"prin-1","created_at":1},"api_key":{"id":"99","name":"Lab API key","key_prefix":"abcdefgh","key":"sk-new"}}}"#;
    let args = [
        "team",
        "create",
        "--name",
        "Lab",
        "--issue-api-key",
        "--idempotency-key",
        "idem-t",
    ];
    let (request, output) = exchange(created, &args);
    let stdout = assert_success(&output, &args);

    assert_eq!(request_line(&request), "POST /admin/v1/teams HTTP/1.1");
    assert_eq!(
        request_body(&request),
        r#"{"name":"Lab","issue_api_key":true}"#
    );
    assert!(
        request
            .to_ascii_lowercase()
            .contains("idempotency-key: idem-t"),
        "{request}"
    );
    assert!(stdout.contains("sk-new"), "{stdout}");
}

#[test]
fn team_create_without_a_key_sends_only_the_name() {
    let created = r#"{"success":true,"data":{"team":{"id":"t-9","name":"Lab","type":"org","owner_principal_id":"prin-1","created_at":1}}}"#;
    let args = ["team", "create", "--name", "Lab"];
    let (request, output) = exchange(created, &args);
    assert_success(&output, &args);

    assert_eq!(request_body(&request), r#"{"name":"Lab"}"#);
}

#[test]
fn team_delete_reads_the_team_then_deletes_it() {
    let deleted = r#"{"success":true,"data":{"tenant_id":"t-1","dissolved_at":1791800000,"subscription_canceled":false}}"#;
    let args = ["team", "delete", "--confirm", "t-1"];
    let (requests, output) = exchange_sequence(&[ORG_TEAM, deleted], &args);
    let stdout = assert_success(&output, &args);

    assert_eq!(request_line(&requests[0]), "GET /admin/v1/team HTTP/1.1");
    assert_eq!(request_line(&requests[1]), "DELETE /admin/v1/team HTTP/1.1");
    assert!(stdout.contains(r#""dissolved_at": 1791800000"#), "{stdout}");
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(stderr.contains("no longer works"), "{stderr}");
}

#[test]
fn team_delete_with_the_wrong_id_never_sends_the_delete() {
    // The stub answers one request only: a DELETE after the GET would fail
    // to connect instead of producing the refusal below.
    let args = ["team", "delete", "--confirm", "t-2"];
    let (request, output) = exchange(ORG_TEAM, &args);
    let err = assert_failure(&output, &args);

    assert_eq!(request_line(&request), "GET /admin/v1/team HTTP/1.1");
    assert!(
        err.contains("does not match") && err.contains("t-1"),
        "{err}"
    );
}

#[test]
fn team_delete_of_a_personal_team_is_refused_locally() {
    let personal = r#"{"success":true,"data":{"id":"630","name":"me","type":"personal","owner_principal_id":"prin-1","created_at":1}}"#;
    let args = ["team", "delete", "--confirm", "630"];
    let (_, output) = exchange(personal, &args);
    let err = assert_failure(&output, &args);

    assert!(err.contains("only org teams"), "{err}");
}

#[test]
fn team_transfer_ownership_confirms_then_posts_the_new_owner() {
    let transferred = r#"{"success":true,"data":{"tenant_id":"t-1","previous_owner_principal_id":"prin-1","new_owner_principal_id":"prin-2"}}"#;
    let args = [
        "team",
        "transfer-ownership",
        "--to",
        "prin-2",
        "--confirm",
        "t-1",
        "--idempotency-key",
        "idem-x",
    ];
    let (requests, output) = exchange_sequence(&[ORG_TEAM, transferred], &args);
    let stdout = assert_success(&output, &args);

    assert_eq!(request_line(&requests[0]), "GET /admin/v1/team HTTP/1.1");
    assert_eq!(
        request_line(&requests[1]),
        "POST /admin/v1/team/transfer-ownership HTTP/1.1"
    );
    assert_eq!(
        request_body(&requests[1]),
        r#"{"new_owner_principal_id":"prin-2"}"#
    );
    assert!(
        requests[1]
            .to_ascii_lowercase()
            .contains("idempotency-key: idem-x"),
        "{}",
        requests[1]
    );
    assert!(
        stdout.contains(r#""previous_owner_principal_id""#),
        "{stdout}"
    );
}
