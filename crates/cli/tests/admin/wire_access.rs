//! Wire-level tests for the access-control commands: `role` writes and
//! scopes, `member scope` / `explain`, `access-group`, `permission`, and
//! `access`.
//!
//! Besides method, path, query, and body, these pin the two headers the
//! permission writes add: `If-Match` (taken from the ETag a read prints on
//! standard error) and `Idempotency-Key`.

use crate::common::stub::{
    exchange, exchange_with_input, exchange_with_response_headers, request_body, request_header,
    request_line,
};
use crate::common::{assert_failure, assert_success};

const EMPTY_PAGE: &str = r#"{"success":true,"message":"Operation completed successfully","data":{"items":[],"total":0}}"#;
const EMPTY_DATA: &str =
    r#"{"success":true,"message":"Operation completed successfully","data":{}}"#;
const RESOURCE_ACCESS: &str = r#"{"success":true,"data":{"resource_type":"memorylake:workspace","instance_id":"ws-1","allowed_count":0,"allowed":[],"blocked":[],"grantable_levels":[]}}"#;
const ROLE: &str = r#"{"success":true,"data":{"key":"tenant_custom_1","label":"Reviewers","description":null,"built_in":false,"assignable":true,"admin_grant_only":false,"base_role_key":"tenant_member","parent_role_key":"tenant_member"}}"#;
const MEMBER_SCOPE: &str = r#"{"success":true,"data":{"principal_id":"prin-1","role":"tenant_member","scopes":[{"resource_type":"memorylake:workspace","mode":"all"}]}}"#;
const GROUP: &str = r#"{"success":true,"data":{"group_id":"g-1","name":"Ops","description":null,"base_role_key":null,"base_access_group_id":null,"permissions":[],"member_count":0,"scope_summary":null,"created_by":"prin-0","created_at":1791800000,"updated_at":1791800000}}"#;
const GROUP_SCOPE: &str = r#"{"success":true,"data":{"group_id":"g-1","scopes":[]}}"#;
const WORKSPACE_SCOPE: &str =
    r#"[{"resource_type":"memorylake:workspace","mode":"instances","instances":[{"id":"ws-1"}]}]"#;

fn stderr(output: &std::process::Output) -> String {
    String::from_utf8_lossy(&output.stderr).into_owned()
}

#[test]
fn role_create_posts_the_label_and_the_base() {
    let args = [
        "role",
        "create",
        "--label",
        "Reviewers",
        "--base-role",
        "tenant_member",
        "--idempotency-key",
        "idem-role-1",
    ];
    let (request, output) = exchange(ROLE, &args);
    let stdout = assert_success(&output, &args);

    assert_eq!(request_line(&request), "POST /admin/v1/roles HTTP/1.1");
    assert_eq!(
        request_body(&request),
        r#"{"label":"Reviewers","base_role_key":"tenant_member"}"#
    );
    assert_eq!(
        request_header(&request, "idempotency-key"),
        Some("idem-role-1")
    );
    assert!(stdout.contains("tenant_custom_1"), "{stdout}");
}

#[test]
fn role_update_with_an_empty_description_clears_it() {
    let args = ["role", "update", "tenant_custom_1", "--description", ""];
    let (request, output) = exchange(ROLE, &args);
    assert_success(&output, &args);

    assert_eq!(
        request_line(&request),
        "PATCH /admin/v1/roles/tenant_custom_1 HTTP/1.1"
    );
    assert_eq!(request_body(&request), r#"{"description":""}"#);
}

#[test]
fn role_delete_deletes_and_confirms() {
    let args = ["role", "delete", "tenant_custom_1"];
    let (request, output) = exchange(EMPTY_DATA, &args);
    let stdout = assert_success(&output, &args);

    assert_eq!(
        request_line(&request),
        "DELETE /admin/v1/roles/tenant_custom_1 HTTP/1.1"
    );
    assert!(stdout.contains("Deleted"), "{stdout}");
}

#[test]
fn scope_get_prints_the_etag_on_standard_error_only() {
    let scope = r#"{"success":true,"data":{"role_key":"tenant_member","scopes":[]}}"#;
    let args = ["role", "scope", "get", "tenant_member"];
    let (request, output) = exchange_with_response_headers(scope, &[("ETag", "\"v7\"")], &args);
    let stdout = assert_success(&output, &args);

    assert_eq!(
        request_line(&request),
        "GET /admin/v1/roles/tenant_member/access-scope HTTP/1.1"
    );
    // Standard output stays pure JSON, so it can be piped back into
    // `scope set --scopes-file -`.
    serde_json::from_str::<serde_json::Value>(&stdout).expect("stdout is JSON");
    assert!(!stdout.contains("v7"), "{stdout}");
    assert!(
        stderr(&output).contains(r#"ETag: "v7""#),
        "{}",
        stderr(&output)
    );
}

#[test]
fn member_scope_set_puts_the_entries_with_the_reason_and_if_match() {
    let args = [
        "member",
        "scope",
        "set",
        "prin-1",
        "--scopes",
        WORKSPACE_SCOPE,
        "--reason",
        "least privilege",
        "--if-match",
        "\"v7\"",
    ];
    let (request, output) = exchange(MEMBER_SCOPE, &args);
    assert_success(&output, &args);

    assert_eq!(
        request_line(&request),
        "PUT /admin/v1/members/prin-1/access-scope HTTP/1.1"
    );
    assert_eq!(
        request_body(&request),
        format!(r#"{{"reason":"least privilege","scopes":{WORKSPACE_SCOPE}}}"#)
    );
    assert_eq!(request_header(&request, "if-match"), Some("\"v7\""));
}

#[test]
fn scope_set_takes_what_scope_get_printed_and_drops_the_read_only_fields() {
    let printed = r#"{"principal_id":"prin-1","role":"tenant_member","scopes":[
        {"resource_type":"memorylake:workspace","mode":"instances","available_actions":["workspace:read"],
         "instances":[{"id":"ws-1","actions":["workspace:read"]}]}]}"#;
    let file = std::env::temp_dir().join(format!("mlcli-scope-{}.json", std::process::id()));
    std::fs::write(&file, printed).unwrap();
    let path = file.to_string_lossy().into_owned();
    let args = [
        "member",
        "scope",
        "set",
        "prin-1",
        "--scopes-file",
        &path,
        "--reason",
        "edit",
    ];
    let (request, output) = exchange(MEMBER_SCOPE, &args);
    let _ = std::fs::remove_file(&file);
    assert_success(&output, &args);

    assert_eq!(
        request_body(&request),
        r#"{"reason":"edit","scopes":[{"resource_type":"memorylake:workspace","mode":"instances","instances":[{"id":"ws-1","actions":["workspace:read"]}]}]}"#
    );
    assert_eq!(request_header(&request, "if-match"), None);
}

#[test]
fn member_explain_reads_the_effective_endpoint() {
    let explained = r#"{"success":true,"data":{"principal_id":"prin-1","display_name":"Ann","role":"tenant_member","permissions":[],"scopes":[]}}"#;
    let args = ["member", "explain", "prin-1"];
    let (request, output) = exchange(explained, &args);
    assert_success(&output, &args);

    assert_eq!(
        request_line(&request),
        "GET /admin/v1/members/prin-1/effective-permissions HTTP/1.1"
    );
}

#[test]
fn access_group_create_splits_each_permission_into_type_and_action() {
    let args = [
        "ag",
        "create",
        "--name",
        "Ops",
        "--base-role",
        "tenant_member",
        "--permission",
        "memorylake:actor:actor:create",
        "--permission",
        "memorylake:skill:skill:read",
    ];
    let (request, output) = exchange(GROUP, &args);
    assert_success(&output, &args);

    assert_eq!(
        request_line(&request),
        "POST /admin/v1/access-groups HTTP/1.1"
    );
    assert_eq!(
        request_body(&request),
        r#"{"name":"Ops","base_role_key":"tenant_member","permissions":[{"resource_type":"memorylake:actor","action":"actor:create"},{"resource_type":"memorylake:skill","action":"skill:read"}]}"#
    );
}

#[test]
fn access_group_get_prints_the_etag() {
    let args = ["access-group", "get", "g-1"];
    let (request, output) = exchange_with_response_headers(GROUP, &[("etag", "\"abc\"")], &args);
    assert_success(&output, &args);

    assert_eq!(
        request_line(&request),
        "GET /admin/v1/access-groups/g-1 HTTP/1.1"
    );
    assert!(
        stderr(&output).contains(r#"ETag: "abc""#),
        "{}",
        stderr(&output)
    );
}

#[test]
fn access_group_permissions_set_puts_the_whole_list_with_if_match() {
    let args = [
        "ag",
        "permissions",
        "set",
        "g-1",
        "--permission",
        "memorylake:agent:agent:read",
        "--reason",
        "on-call",
        "--if-match",
        "\"abc\"",
    ];
    let (request, output) = exchange(GROUP, &args);
    assert_success(&output, &args);

    assert_eq!(
        request_line(&request),
        "PUT /admin/v1/access-groups/g-1/permissions HTTP/1.1"
    );
    assert_eq!(
        request_body(&request),
        r#"{"permissions":[{"resource_type":"memorylake:agent","action":"agent:read"}],"reason":"on-call"}"#
    );
    assert_eq!(request_header(&request, "if-match"), Some("\"abc\""));
}

#[test]
fn access_group_permissions_set_none_sends_an_empty_list() {
    let args = [
        "ag",
        "permissions",
        "set",
        "g-1",
        "--none",
        "--reason",
        "off-boarding",
    ];
    let (request, output) = exchange(GROUP, &args);
    assert_success(&output, &args);

    assert_eq!(
        request_body(&request),
        r#"{"permissions":[],"reason":"off-boarding"}"#
    );
}

#[test]
fn access_group_member_add_posts_every_id() {
    let args = ["ag", "member", "add", "g-1", "prin-1", "prin-2"];
    let (request, output) = exchange(r#"{"success":true,"data":{"added":2}}"#, &args);
    assert_success(&output, &args);

    assert_eq!(
        request_line(&request),
        "POST /admin/v1/access-groups/g-1/members HTTP/1.1"
    );
    assert_eq!(
        request_body(&request),
        r#"{"principal_ids":["prin-1","prin-2"]}"#
    );
}

#[test]
fn access_group_member_remove_deletes_the_one_member() {
    let args = ["ag", "member", "remove", "g-1", "prin-2"];
    let (request, output) = exchange(EMPTY_DATA, &args);
    assert_success(&output, &args);

    assert_eq!(
        request_line(&request),
        "DELETE /admin/v1/access-groups/g-1/members/prin-2 HTTP/1.1"
    );
}

#[test]
fn access_group_scope_and_explain_reach_their_endpoints() {
    let args = [
        "ag",
        "scope",
        "set",
        "g-1",
        "--scopes",
        WORKSPACE_SCOPE,
        "--reason",
        "r",
    ];
    let (request, output) = exchange(GROUP_SCOPE, &args);
    assert_success(&output, &args);
    assert_eq!(
        request_line(&request),
        "PUT /admin/v1/access-groups/g-1/access-scope HTTP/1.1"
    );

    let explained = r#"{"success":true,"data":{"group_id":"g-1","name":"Ops","member_count":0,"inherits_from":null,"permissions":[],"scopes":[]}}"#;
    let (request, output) = exchange(explained, &["ag", "explain", "g-1"]);
    assert_success(&output, &["ag", "explain", "g-1"]);
    assert_eq!(
        request_line(&request),
        "GET /admin/v1/access-groups/g-1/effective-permissions HTTP/1.1"
    );
}

#[test]
fn permission_catalog_for_access_groups_filters_on_the_server() {
    let catalog = r#"{"success":true,"data":{"resource_types":[]}}"#;
    let args = ["perm", "catalog", "--access-group"];
    let (request, output) = exchange(catalog, &args);
    assert_success(&output, &args);

    assert_eq!(
        request_line(&request),
        "GET /admin/v1/permission-catalog?grantable_to=access_group HTTP/1.1"
    );
}

#[test]
fn permission_me_reads_the_callers_permissions() {
    let mine = r#"{"success":true,"data":{"role":"tenant_owner","permissions":[]}}"#;
    let (request, output) = exchange(mine, &["permission", "me"]);
    assert_success(&output, &["permission", "me"]);

    assert_eq!(
        request_line(&request),
        "GET /admin/v1/me/permissions HTTP/1.1"
    );
}

#[test]
fn permission_rule_list_filters_ride_the_query_string() {
    let args = [
        "perm",
        "rule",
        "list",
        "--subject-type",
        "role",
        "--subject-id",
        "tenant_custom_1",
        "--page-size",
        "10",
    ];
    let (request, output) = exchange(EMPTY_PAGE, &args);
    assert_success(&output, &args);

    assert_eq!(
        request_line(&request),
        "GET /admin/v1/permission-rules?subject_type=role&subject_id=tenant_custom_1&page_size=10 HTTP/1.1"
    );
}

#[test]
fn permission_rule_create_sends_one_request_for_every_action() {
    let created = r#"{"success":true,"data":{"rules":[],"prerequisite_rule_count":0}}"#;
    let args = [
        "perm",
        "rule",
        "create",
        "--member",
        "prin-1",
        "--resource",
        "memorylake:skill",
        "--action",
        "skill:read,skill:modify",
        "--auto-grant-prerequisites",
        "--reason",
        "maintainer",
    ];
    let (request, output) = exchange(created, &args);
    assert_success(&output, &args);

    assert_eq!(
        request_line(&request),
        "POST /admin/v1/permission-rules HTTP/1.1"
    );
    assert_eq!(
        request_body(&request),
        r#"{"subject_type":"member","subject_id":"prin-1","resource":"memorylake:skill","actions":["skill:read","skill:modify"],"auto_grant_prerequisites":true,"reason":"maintainer"}"#
    );
}

#[test]
fn permission_rule_delete_carries_the_reason_in_the_query() {
    let args = ["perm", "rule", "delete", "pr1_abc", "--reason", "cleanup"];
    let (request, output) = exchange(EMPTY_DATA, &args);
    let stdout = assert_success(&output, &args);

    assert_eq!(
        request_line(&request),
        "DELETE /admin/v1/permission-rules/pr1_abc?reason=cleanup HTTP/1.1"
    );
    assert!(stdout.contains("Deleted"), "{stdout}");
}

#[test]
fn access_show_reads_the_instance() {
    for (args, path) in [
        (
            vec!["access", "show", "workspace", "ws-1"],
            "/admin/v1/access/workspaces/ws-1",
        ),
        (
            vec!["access", "show", "db-connection", "conn-1"],
            "/admin/v1/access/db-connections/conn-1",
        ),
        (
            vec![
                "access",
                "show",
                "workspace-agent",
                "ag-1",
                "--workspace",
                "ws-1",
            ],
            "/admin/v1/access/workspaces/ws-1/agents/ag-1",
        ),
    ] {
        let (request, output) = exchange(RESOURCE_ACCESS, &args);
        assert_success(&output, &args);
        assert_eq!(request_line(&request), format!("GET {path} HTTP/1.1"));
    }
}

#[test]
fn access_levels_reads_the_member_options() {
    let options = r#"{"success":true,"data":{"current_level":null,"settable_levels":["read"],"can_set_none":false,"blocked_reason":null}}"#;
    let args = ["access", "levels", "skill", "sk-1", "--member", "prin-1"];
    let (request, output) = exchange(options, &args);
    assert_success(&output, &args);

    assert_eq!(
        request_line(&request),
        "GET /admin/v1/access/skills/sk-1/permission-levels/members/prin-1/options HTTP/1.1"
    );
}

#[test]
fn access_set_level_on_a_project_names_its_workspace_in_the_body() {
    let args = [
        "access",
        "set-level",
        "project",
        "proj-1",
        "--workspace",
        "ws-1",
        "--member",
        "prin-1",
        "--level",
        "write",
        "--reason",
        "editor",
        "--idempotency-key",
        "idem-lvl-1",
    ];
    let (request, output) = exchange(RESOURCE_ACCESS, &args);
    assert_success(&output, &args);

    assert_eq!(
        request_line(&request),
        "PUT /admin/v1/access/projects/proj-1/permission-levels/members/prin-1 HTTP/1.1"
    );
    assert_eq!(
        request_body(&request),
        r#"{"level":"write","reason":"editor","workspace_id":"ws-1"}"#
    );
    assert_eq!(
        request_header(&request, "idempotency-key"),
        Some("idem-lvl-1")
    );
}

#[test]
fn access_grant_and_revoke_address_the_group_or_member_grant() {
    let args = [
        "access", "grant", "actor", "act-1", "--group", "g-1", "--reason", "ops",
    ];
    let (request, output) = exchange(RESOURCE_ACCESS, &args);
    assert_success(&output, &args);
    assert_eq!(
        request_line(&request),
        "PUT /admin/v1/access/actors/act-1/scope-grants/access-groups/g-1 HTTP/1.1"
    );
    assert_eq!(request_body(&request), r#"{"reason":"ops"}"#);

    let args = [
        "access", "revoke", "actor", "act-1", "--member", "prin-1", "--reason", "gone",
    ];
    let (request, output) = exchange(RESOURCE_ACCESS, &args);
    assert_success(&output, &args);
    assert_eq!(
        request_line(&request),
        "DELETE /admin/v1/access/actors/act-1/scope-grants/members/prin-1?reason=gone HTTP/1.1"
    );
}

#[test]
fn a_stale_etag_surfaces_the_servers_412() {
    let stale = r#"{"success":false,"message":"the resource has changed since it was read; read it again and retry with the new ETag","error_code":"STATE_NOT_READY"}"#;
    let args = [
        "ag",
        "permissions",
        "set",
        "g-1",
        "--none",
        "--reason",
        "r",
        "--if-match",
        "\"old\"",
    ];
    let (_, output) = exchange(stale, &args);
    let err = assert_failure(&output, &args);
    assert!(err.contains("read it again"), "{err}");
}

#[test]
fn role_get_reads_one_role() {
    let args = ["role", "get", "tenant_custom_1"];
    let (request, output) = exchange(ROLE, &args);
    assert_success(&output, &args);
    assert_eq!(
        request_line(&request),
        "GET /admin/v1/roles/tenant_custom_1 HTTP/1.1"
    );
}

#[test]
fn role_scope_set_puts_the_entries_with_the_reason() {
    let scope = r#"{"success":true,"data":{"role_key":"tenant_custom_1","scopes":[]}}"#;
    let args = [
        "role",
        "scope",
        "set",
        "tenant_custom_1",
        "--scopes",
        WORKSPACE_SCOPE,
        "--reason",
        "contractors",
        "--if-match",
        "\"v3\"",
    ];
    let (request, output) = exchange(scope, &args);
    assert_success(&output, &args);

    assert_eq!(
        request_line(&request),
        "PUT /admin/v1/roles/tenant_custom_1/access-scope HTTP/1.1"
    );
    assert_eq!(
        request_body(&request),
        r#"{"reason":"contractors","scopes":[{"resource_type":"memorylake:workspace","mode":"instances","instances":[{"id":"ws-1"}]}]}"#
    );
    assert_eq!(request_header(&request, "if-match"), Some("\"v3\""));
}

#[test]
fn member_scope_get_prints_the_etag() {
    let args = ["member", "scope", "get", "prin-1"];
    let (request, output) =
        exchange_with_response_headers(MEMBER_SCOPE, &[("ETag", "\"m1\"")], &args);
    assert_success(&output, &args);

    assert_eq!(
        request_line(&request),
        "GET /admin/v1/members/prin-1/access-scope HTTP/1.1"
    );
    assert!(
        stderr(&output).contains(r#"ETag: "m1""#),
        "{}",
        stderr(&output)
    );
}

#[test]
fn access_group_list_pages_through_the_groups() {
    let args = [
        "access-group",
        "list",
        "--page-size",
        "10",
        "--continuation-token",
        "tok",
    ];
    let (request, output) = exchange(EMPTY_PAGE, &args);
    assert_success(&output, &args);
    assert_eq!(
        request_line(&request),
        "GET /admin/v1/access-groups?page_size=10&continuation_token=tok HTTP/1.1"
    );
}

#[test]
fn access_group_update_patches_only_the_given_fields() {
    let args = [
        "ag",
        "update",
        "g-1",
        "--description",
        "",
        "--idempotency-key",
        "idem-ag-1",
    ];
    let (request, output) = exchange(GROUP, &args);
    assert_success(&output, &args);

    assert_eq!(
        request_line(&request),
        "PATCH /admin/v1/access-groups/g-1 HTTP/1.1"
    );
    assert_eq!(request_body(&request), r#"{"description":""}"#);
    assert_eq!(
        request_header(&request, "idempotency-key"),
        Some("idem-ag-1")
    );
}

#[test]
fn access_group_delete_deletes_and_confirms() {
    let args = ["ag", "delete", "g-1"];
    let (request, output) = exchange(EMPTY_DATA, &args);
    let stdout = assert_success(&output, &args);

    assert_eq!(
        request_line(&request),
        "DELETE /admin/v1/access-groups/g-1 HTTP/1.1"
    );
    assert!(stdout.contains("Deleted"), "{stdout}");
}

#[test]
fn access_group_member_list_and_scope_get_read_their_endpoints() {
    let args = ["ag", "member", "list", "g-1", "--page-size", "5"];
    let (request, output) = exchange(EMPTY_PAGE, &args);
    assert_success(&output, &args);
    assert_eq!(
        request_line(&request),
        "GET /admin/v1/access-groups/g-1/members?page_size=5 HTTP/1.1"
    );

    let args = ["ag", "scope", "get", "g-1"];
    let (request, output) =
        exchange_with_response_headers(GROUP_SCOPE, &[("ETag", "\"s1\"")], &args);
    assert_success(&output, &args);
    assert_eq!(
        request_line(&request),
        "GET /admin/v1/access-groups/g-1/access-scope HTTP/1.1"
    );
    assert!(
        stderr(&output).contains(r#"ETag: "s1""#),
        "{}",
        stderr(&output)
    );
}

#[test]
fn an_etag_whose_quotes_the_shell_ate_is_quoted_again() {
    // `--if-match "abc"` reaches the CLI as `abc`; the server compares the
    // tag quotes included, so it must go out as `"abc"`. A pasted
    // `ETag: "abc"` line works as well.
    for given in ["abc", "ETag: \"abc\""] {
        let args = [
            "ag",
            "permissions",
            "set",
            "g-1",
            "--none",
            "--reason",
            "r",
            "--if-match",
            given,
        ];
        let (request, output) = exchange(GROUP, &args);
        assert_success(&output, &args);
        assert_eq!(request_header(&request, "if-match"), Some("\"abc\""));
    }
}

#[test]
fn a_workspace_binding_defaults_to_the_remembered_workspace() {
    let args = ["access", "show", "workspace-actor", "act-1"];
    let (request, output) = exchange_with_input(
        RESOURCE_ACCESS,
        &args,
        "",
        &[("MEMORYLAKE_WORKSPACE", "ws-9")],
    );
    assert_success(&output, &args);
    assert_eq!(
        request_line(&request),
        "GET /admin/v1/access/workspaces/ws-9/actors/act-1 HTTP/1.1"
    );

    // A project's workspace only labels it, so it is never guessed.
    let args = ["access", "show", "project", "p-1"];
    let (request, output) = exchange_with_input(
        RESOURCE_ACCESS,
        &args,
        "",
        &[("MEMORYLAKE_WORKSPACE", "ws-9")],
    );
    assert_success(&output, &args);
    assert_eq!(
        request_line(&request),
        "GET /admin/v1/access/projects/p-1 HTTP/1.1"
    );
}
