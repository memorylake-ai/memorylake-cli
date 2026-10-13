//! Wire-level tests for the remembered workspace.
//!
//! `workspace use` is only useful if the id it stores actually reaches the
//! request, and only safe if an explicit `--workspace` still wins. Both are
//! invisible to a unit test of the resolver — they depend on every command
//! routing its flag through it — so they are pinned here, on the URL.

use crate::common::stub::{
    exchange, exchange_sequence_with_remembered_workspace, exchange_with_remembered_workspace,
    request_body, request_line,
};
use crate::common::{assert_failure, assert_success};

const EMPTY_PAGE: &str = r#"{"success":true,"data":{"items":[]}}"#;
const WORKSPACE: &str = r#"{"success":true,"data":{"id":"ws-remembered","name":"Remembered"}}"#;

#[test]
fn a_remembered_workspace_reaches_the_request_url() {
    let (request, output) =
        exchange_with_remembered_workspace(EMPTY_PAGE, "ws-remembered", &["project", "list"]);
    assert_success(&output, &["project", "list"]);

    assert_eq!(
        request_line(&request),
        "GET /api/v3/workspaces/ws-remembered/projects HTTP/1.1",
        "`project list` with no --workspace must use the remembered one"
    );
}

#[test]
fn an_explicit_workspace_outranks_the_remembered_one() {
    let (request, output) = exchange_with_remembered_workspace(
        EMPTY_PAGE,
        "ws-remembered",
        &["project", "list", "--workspace", "ws-explicit"],
    );
    assert_success(&output, &["project", "list"]);

    assert_eq!(
        request_line(&request),
        "GET /api/v3/workspaces/ws-explicit/projects HTTP/1.1",
        "an explicit --workspace must win over the remembered one"
    );
}

#[test]
fn every_workspace_scoped_command_family_honours_the_remembered_one() {
    // The resolver is shared, but each command has to call it. One command per
    // family, so a family that forgets to is caught here rather than by a user.
    for (args, expected) in [
        (
            vec!["project", "list"],
            "GET /api/v3/workspaces/ws-remembered/projects HTTP/1.1",
        ),
        (
            vec!["project", "document", "list", "--project", "proj-1"],
            "GET /api/v3/workspaces/ws-remembered/projects/proj-1/memories/documents HTTP/1.1",
        ),
        (
            vec!["fact", "list", "--actors", "actor-1"],
            "GET /api/v3/workspaces/ws-remembered/memories/facts?actor_ids=actor-1 HTTP/1.1",
        ),
        (
            vec!["conversation", "list"],
            "GET /api/v3/workspaces/ws-remembered/memories/conversations HTTP/1.1",
        ),
        (
            vec!["agent", "bindings"],
            "GET /api/v3/workspaces/ws-remembered/agents HTTP/1.1",
        ),
    ] {
        let (request, output) =
            exchange_with_remembered_workspace(EMPTY_PAGE, "ws-remembered", &args);
        assert_success(&output, &args);
        assert_eq!(
            request_line(&request),
            expected,
            "{args:?} did not use the remembered workspace"
        );
    }
}

#[test]
fn workspace_use_verifies_the_id_before_remembering_it() {
    // Storing an id the API rejects would turn one clear error now into a
    // confusing one on every later command, so `use <id>` looks it up first.
    let (request, output) = exchange(WORKSPACE, &["workspace", "use", "ws-remembered"]);
    let stdout = assert_success(&output, &["workspace", "use"]);

    assert_eq!(
        request_line(&request),
        "GET /api/v3/workspaces/ws-remembered HTTP/1.1",
        "the id is looked up before being stored"
    );
    assert!(
        stdout.contains("ws-remembered"),
        "the command confirms what it stored: {stdout}"
    );
}

#[test]
fn update_patches_only_the_fields_given() {
    let args = [
        "workspace",
        "update",
        "ws-1",
        "--description",
        "",
        "--metadata",
        r#"{"team":"core"}"#,
    ];
    let (request, output) = exchange(WORKSPACE, &args);
    let stdout = assert_success(&output, &args);

    assert_eq!(
        request_line(&request),
        "PATCH /api/v3/workspaces/ws-1 HTTP/1.1"
    );
    let body: serde_json::Value =
        serde_json::from_str(request_body(&request)).expect("body is JSON");
    assert_eq!(
        body,
        serde_json::json!({"description": "", "metadata": {"team": "core"}}),
        "an empty description is an instruction to clear, and name was not given"
    );
    assert!(stdout.contains("ws-remembered"), "{stdout}");
}

#[test]
fn update_sends_an_empty_metadata_object_to_clear_it() {
    let args = ["workspace", "update", "ws-1", "--metadata", "{}"];
    let (request, output) = exchange(WORKSPACE, &args);
    assert_success(&output, &args);
    assert_eq!(request_body(&request), r#"{"metadata":{}}"#);
}

const DELETED: &str = r#"{"success":true,"data":{}}"#;
const SCRATCH: &str =
    r#"{"success":true,"data":{"id":"ws-scratch","name":"Scratch space","custom_id":"scratch"}}"#;

#[test]
fn delete_reads_the_name_back_then_deletes_and_forgets_it() {
    let args = [
        "workspace",
        "delete",
        "ws-scratch",
        "--confirm",
        "Scratch space",
    ];
    let (requests, output, config) =
        exchange_sequence_with_remembered_workspace(&[SCRATCH, DELETED], "ws-scratch", &args);
    let stdout = assert_success(&output, &args);

    assert_eq!(
        request_line(&requests[0]),
        "GET /api/v3/workspaces/ws-scratch HTTP/1.1",
        "the name is read back before anything is deleted"
    );
    assert_eq!(
        request_line(&requests[1]),
        "DELETE /api/v3/workspaces/ws-scratch HTTP/1.1"
    );
    assert!(
        stdout.contains("Deleted workspace `ws-scratch` (Scratch space)"),
        "{stdout}"
    );
    assert!(
        stdout.contains("profile `default` no longer remembers it"),
        "{stdout}"
    );
    assert!(
        !config.contains("ws-scratch"),
        "a deleted workspace must not stay remembered:\n{config}"
    );
}

#[test]
fn delete_keeps_a_different_remembered_workspace() {
    let args = [
        "workspace",
        "delete",
        "ws-scratch",
        "--confirm",
        "Scratch space",
    ];
    let (_, output, config) =
        exchange_sequence_with_remembered_workspace(&[SCRATCH, DELETED], "ws-other", &args);
    let stdout = assert_success(&output, &args);
    assert!(!stdout.contains("no longer remembers"), "{stdout}");
    assert!(config.contains("ws-other"), "{config}");
}

#[test]
fn delete_refuses_a_name_that_does_not_match() {
    // One canned response: a DELETE after the GET would find no server and
    // fail with a connection error, not with the refusal asserted here.
    for confirm in ["Scratch", "scratch space", "Scratch space ", "ws-scratch"] {
        let args = ["workspace", "delete", "ws-scratch", "--confirm", confirm];
        let (request, output) = exchange(SCRATCH, &args);
        let err = assert_failure(&output, &args);
        assert_eq!(
            request_line(&request),
            "GET /api/v3/workspaces/ws-scratch HTTP/1.1"
        );
        assert!(
            err.contains("refusing to delete `ws-scratch`") && err.contains("\"Scratch space\""),
            "{confirm:?}: {err}"
        );
    }
}

#[test]
fn delete_refuses_the_default_workspace_even_when_named() {
    let default = r#"{"success":true,"data":{"id":"ws-default","name":"Default Workspace","custom_id":"_sys_default_workspace"}}"#;
    let args = [
        "workspace",
        "delete",
        "ws-default",
        "--confirm",
        "Default Workspace",
    ];
    let (_, output) = exchange(default, &args);
    let err = assert_failure(&output, &args);
    assert!(err.contains("default workspace"), "{err}");
}
