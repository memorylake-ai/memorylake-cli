//! Offline `conversation` tests (no network; temp `$HOME` only).

use std::fs;

use crate::common::stub::logged_in_home;
use crate::common::{assert_failure, assert_success, run, temp_home};

/// A base URL nothing is listening on.
///
/// Local validation must fail *before* the CLI tries to reach this, so a test
/// that sees a connection error here has caught validation happening too late.
const UNREACHABLE: &str = "http://127.0.0.1:1/openapi/memorylake";

/// Assert the command failed locally rather than by contacting the network.
fn assert_rejected_locally(args: &[&str], expected: &str) {
    let home = logged_in_home(UNREACHABLE);
    let err = assert_failure(&run(&home, args), args);
    assert!(
        err.contains(expected),
        "expected {expected:?} for {args:?}, got: {err}"
    );
    assert!(
        !err.contains("could not connect") && !err.contains("timed out"),
        "validation must happen before any request, but {args:?} reached the network: {err}"
    );
    let _ = fs::remove_dir_all(&home);
}

#[test]
fn help_lists_conversation_subcommands() {
    let home = temp_home();
    let args = ["conversation", "--help"];
    let stdout = assert_success(&run(&home, &args), &args);
    for subcommand in ["create", "list", "get", "delete", "cook-status", "message"] {
        assert!(
            stdout.contains(subcommand),
            "`conversation --help` missing {subcommand}: {stdout}"
        );
    }
    let _ = fs::remove_dir_all(&home);
}

#[test]
fn the_conv_alias_reaches_the_same_command() {
    let home = temp_home();
    let args = ["conv", "--help"];
    let stdout = assert_success(&run(&home, &args), &args);
    assert!(stdout.contains("cook-status"), "conv alias: {stdout}");
    let _ = fs::remove_dir_all(&home);
}

#[test]
fn message_help_lists_its_subcommands() {
    let home = temp_home();
    let args = ["conversation", "message", "--help"];
    let stdout = assert_success(&run(&home, &args), &args);
    for subcommand in ["append", "list"] {
        assert!(
            stdout.contains(subcommand),
            "`conversation message --help` missing {subcommand}: {stdout}"
        );
    }
    let _ = fs::remove_dir_all(&home);
}

#[test]
fn create_requires_a_workspace_a_custom_id_a_project_and_actors() {
    let home = temp_home();
    for (args, expected) in [
        (
            vec![
                "conversation",
                "create",
                "--custom-id",
                "s-1",
                "--project",
                "proj-1",
                "--actors",
                "a-1",
            ],
            "--workspace",
        ),
        (
            vec![
                "conversation",
                "create",
                "--workspace",
                "ws-1",
                "--project",
                "proj-1",
                "--actors",
                "a-1",
            ],
            "--custom-id",
        ),
        (
            vec![
                "conversation",
                "create",
                "--workspace",
                "ws-1",
                "--custom-id",
                "s-1",
                "--actors",
                "a-1",
            ],
            "--project",
        ),
        (
            vec![
                "conversation",
                "create",
                "--workspace",
                "ws-1",
                "--custom-id",
                "s-1",
                "--project",
                "proj-1",
            ],
            "--actors",
        ),
    ] {
        let err = assert_failure(&run(&home, &args), &args);
        assert!(
            err.contains(expected),
            "expected {expected} to be required: {err}"
        );
    }
    let _ = fs::remove_dir_all(&home);
}

#[test]
fn create_rejects_a_kind_outside_the_documented_pair() {
    assert_rejected_locally(
        &[
            "conversation",
            "create",
            "--workspace",
            "ws-1",
            "--custom-id",
            "s-1",
            "--project",
            "proj-1",
            "--actors",
            "a-1",
            "--kind",
            "BROADCAST",
        ],
        "invalid value",
    );
}

#[test]
fn create_spells_kinds_the_way_the_api_does() {
    // Closed and case-sensitive, like `actor --type`: the uppercase forms are
    // the only ones accepted. Reaching the network means the value parsed —
    // the unreachable URL then fails the command.
    let home = logged_in_home(UNREACHABLE);
    for kind in ["DIRECT", "GROUP"] {
        let args = [
            "conversation",
            "create",
            "--workspace",
            "ws-1",
            "--custom-id",
            "s-1",
            "--project",
            "proj-1",
            "--actors",
            "a-1",
            "--kind",
            kind,
        ];
        let err = assert_failure(&run(&home, &args), &args);
        assert!(
            !err.contains("invalid value"),
            "`--kind {kind}` must parse: {err}"
        );
    }
    let _ = fs::remove_dir_all(&home);

    assert_rejected_locally(
        &[
            "conversation",
            "create",
            "--workspace",
            "ws-1",
            "--custom-id",
            "s-1",
            "--project",
            "proj-1",
            "--actors",
            "a-1",
            "--kind",
            "direct",
        ],
        "invalid value",
    );
}

#[test]
fn metadata_must_be_key_equals_value() {
    assert_rejected_locally(
        &[
            "conversation",
            "create",
            "--workspace",
            "ws-1",
            "--custom-id",
            "s-1",
            "--project",
            "proj-1",
            "--actors",
            "a-1",
            "--metadata",
            "novalue",
        ],
        "expected `key=value`",
    );
    assert_rejected_locally(
        &[
            "conversation",
            "create",
            "--workspace",
            "ws-1",
            "--custom-id",
            "s-1",
            "--project",
            "proj-1",
            "--actors",
            "a-1",
            "--metadata",
            "=orphan",
        ],
        "must not be empty",
    );
}

#[test]
fn message_append_requires_content() {
    assert_rejected_locally(
        &[
            "conversation",
            "message",
            "append",
            "conv-1",
            "--actor",
            "actor-1",
            "--custom-id",
            "msg-1",
        ],
        "message content is required",
    );
}

#[test]
fn message_append_rejects_two_sources_of_content() {
    assert_rejected_locally(
        &[
            "conversation",
            "message",
            "append",
            "conv-1",
            "--actor",
            "actor-1",
            "--custom-id",
            "msg-1",
            "--text",
            "hello",
            "--content-json",
            r#"[{"block_type":"TEXT","text":"x"}]"#,
        ],
        "pass only one",
    );
}

#[test]
fn message_append_rejects_malformed_content_before_sending_it() {
    assert_rejected_locally(
        &[
            "conversation",
            "message",
            "append",
            "conv-1",
            "--actor",
            "actor-1",
            "--custom-id",
            "msg-1",
            "--content-json",
            r#"{"block_type":"TEXT","text":"x"}"#,
        ],
        "array of content blocks",
    );
    assert_rejected_locally(
        &[
            "conversation",
            "message",
            "append",
            "conv-1",
            "--actor",
            "actor-1",
            "--custom-id",
            "msg-1",
            "--content-json",
            r#"[{"text":"no type"}]"#,
        ],
        "block_type",
    );
}

#[test]
fn message_append_wait_requires_a_workspace() {
    // `cook-status` is workspace-scoped while `message append` is not, so
    // --wait needs a workspace the command otherwise never asks for. clap
    // catches it before any request.
    let home = temp_home();
    let args = [
        "conversation",
        "message",
        "append",
        "conv-1",
        "--actor",
        "actor-1",
        "--custom-id",
        "msg-1",
        "--text",
        "hi",
        "--wait",
    ];
    let err = assert_failure(&run(&home, &args), &args);
    assert!(
        err.contains("--workspace"),
        "--wait must name what it is missing: {err}"
    );
    let _ = fs::remove_dir_all(&home);
}

#[test]
fn message_append_requires_an_actor_and_a_custom_id() {
    let home = temp_home();
    for (args, expected) in [
        (
            vec![
                "conversation",
                "message",
                "append",
                "conv-1",
                "--custom-id",
                "msg-1",
                "--text",
                "hi",
            ],
            "--actor",
        ),
        (
            vec![
                "conversation",
                "message",
                "append",
                "conv-1",
                "--actor",
                "actor-1",
                "--text",
                "hi",
            ],
            "--custom-id",
        ),
    ] {
        let err = assert_failure(&run(&home, &args), &args);
        assert!(
            err.contains(expected),
            "expected {expected} to be required: {err}"
        );
    }
    let _ = fs::remove_dir_all(&home);
}

#[test]
fn every_subcommand_without_login_fails() {
    let home = temp_home();
    for args in [
        vec![
            "conversation",
            "create",
            "--workspace",
            "ws-1",
            "--custom-id",
            "s-1",
            "--project",
            "proj-1",
            "--actors",
            "a-1",
        ],
        vec!["conversation", "list", "--workspace", "ws-1"],
        vec!["conversation", "get", "--workspace", "ws-1", "conv-1"],
        vec!["conversation", "delete", "--workspace", "ws-1", "conv-1"],
        vec![
            "conversation",
            "cook-status",
            "--workspace",
            "ws-1",
            "conv-1",
        ],
        vec![
            "conversation",
            "message",
            "append",
            "conv-1",
            "--actor",
            "actor-1",
            "--custom-id",
            "msg-1",
            "--text",
            "hi",
        ],
        vec!["conversation", "message", "list", "conv-1"],
    ] {
        let err = assert_failure(&run(&home, &args), &args);
        assert!(
            err.contains("not logged in"),
            "expected login error for {args:?}: {err}"
        );
    }
    let _ = fs::remove_dir_all(&home);
}
