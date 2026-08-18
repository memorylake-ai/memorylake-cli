//! Wire-level tests for the remembered workspace.
//!
//! `workspace use` is only useful if the id it stores actually reaches the
//! request, and only safe if an explicit `--workspace` still wins. Both are
//! invisible to a unit test of the resolver — they depend on every command
//! routing its flag through it — so they are pinned here, on the URL.

use crate::common::assert_success;
use crate::common::stub::{exchange, exchange_with_remembered_workspace, request_line};

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
