//! Offline `actor` tests (no network; temp `$HOME` only).

use std::fs;

use crate::common::{assert_failure, run, temp_home};

/// Every actor subcommand must refuse to run without resolvable credentials.
#[test]
fn subcommands_without_login_fail() {
    let home = temp_home();
    let cases: [&[&str]; 7] = [
        &["actor", "list"],
        &[
            "actor",
            "create",
            "--custom-id",
            "u-1",
            "--display-name",
            "U",
        ],
        &["actor", "get", "act-does-not-matter"],
        &["actor", "update", "act-1", "--description", "x"],
        &["actor", "delete", "act-1"],
        &["actor", "bind", "--workspace", "ws-1", "--actor", "act-1"],
        &["actor", "unbind", "--workspace", "ws-1", "--actor", "act-1"],
    ];

    for args in cases {
        let err = assert_failure(&run(&home, args), args);
        assert!(
            err.contains("not logged in") || err.contains("resolve API credentials"),
            "unexpected error output for {args:?}: {err}"
        );
    }

    let _ = fs::remove_dir_all(&home);
}

#[test]
fn list_in_workspace_without_login_fails() {
    let home = temp_home();
    let args = ["actor", "list", "--workspace", "ws-1"];
    let err = assert_failure(&run(&home, &args), &args);
    assert!(
        err.contains("not logged in") || err.contains("resolve API credentials"),
        "unexpected error output: {err}"
    );
    let _ = fs::remove_dir_all(&home);
}

#[test]
fn update_without_any_field_fails_before_credentials() {
    let home = temp_home();
    let args = ["actor", "update", "act-1"];
    let err = assert_failure(&run(&home, &args), &args);
    assert!(
        err.contains(
            "at least one of --display-name, --description, --tags, --clear-tags, or --metadata"
        ),
        "unexpected error output: {err}"
    );
    // The input error must win over the not-logged-in error, otherwise the
    // check is only reachable for authenticated users.
    assert!(
        !err.contains("not logged in"),
        "input validation must run before credential resolution: {err}"
    );
    let _ = fs::remove_dir_all(&home);
}

#[test]
fn tags_and_clear_tags_together_are_rejected() {
    let home = temp_home();
    let args = ["actor", "update", "act-1", "--tags", "vip", "--clear-tags"];
    let err = assert_failure(&run(&home, &args), &args);
    assert!(
        err.contains("--clear-tags") && err.contains("--tags"),
        "the error must name both conflicting flags: {err}"
    );
    let _ = fs::remove_dir_all(&home);
}

#[test]
fn tags_must_not_be_empty_or_have_doubled_commas() {
    let home = temp_home();
    // `--tags ""` is rejected rather than treated as "clear": clearing has its
    // own flag, and an empty value is far more likely to be a shell mishap.
    for raw in ["", "   ", "vip,,cn", "vip,"] {
        let args = [
            "actor",
            "create",
            "--custom-id",
            "u-1",
            "--display-name",
            "U",
            "--tags",
            raw,
        ];
        let err = assert_failure(&run(&home, &args), &args);
        assert!(
            err.contains("must not be empty") || err.contains("empty entry"),
            "unexpected error output for {raw:?}: {err}"
        );
        assert!(
            !err.contains("not logged in"),
            "tags must be rejected before credential resolution: {err}"
        );
    }
    let _ = fs::remove_dir_all(&home);
}

#[test]
fn metadata_must_be_valid_json() {
    let home = temp_home();
    let args = [
        "actor",
        "create",
        "--custom-id",
        "u-1",
        "--display-name",
        "U",
        "--metadata",
        "not json",
    ];
    let err = assert_failure(&run(&home, &args), &args);
    assert!(err.contains("must be a JSON object"), "{err}");
    assert!(err.contains("invalid JSON"), "{err}");
    assert!(
        !err.contains("not logged in"),
        "metadata must be rejected before credential resolution: {err}"
    );
    let _ = fs::remove_dir_all(&home);
}

#[test]
fn metadata_must_be_a_json_object() {
    let home = temp_home();
    for (raw, kind) in [
        ("[1,2]", "an array"),
        ("\"text\"", "a string"),
        ("42", "a number"),
        ("null", "null"),
    ] {
        let args = ["actor", "update", "act-1", "--metadata", raw];
        let err = assert_failure(&run(&home, &args), &args);
        assert!(
            err.contains(&format!("must be a JSON object, got {kind}")),
            "unexpected error output for {raw}: {err}"
        );
    }
    let _ = fs::remove_dir_all(&home);
}

#[test]
fn actor_type_flag_rejects_unknown_values() {
    let home = temp_home();
    // Lower-case is a typo, not a synonym: it must never reach the server.
    for raw in ["human", "Human", "ROBOT"] {
        let args = ["actor", "list", "--type", raw];
        let err = assert_failure(&run(&home, &args), &args);
        assert!(
            err.contains(raw) && err.contains("HUMAN") && err.contains("ASSISTANT"),
            "error should show the invalid value and the allowed ones, got: {err}"
        );
        assert!(
            !err.contains("not logged in"),
            "invalid --type must be rejected before credential resolution: {err}"
        );
    }
    let _ = fs::remove_dir_all(&home);
}

#[test]
fn help_lists_actor_subcommands() {
    let home = temp_home();
    let args = ["actor", "--help"];
    let output = run(&home, &args);
    let stdout = String::from_utf8_lossy(&output.stdout).into_owned();
    assert!(output.status.success(), "actor --help failed: {stdout}");
    for subcommand in [
        "list", "create", "get", "update", "delete", "bind", "unbind",
    ] {
        assert!(
            stdout.contains(subcommand),
            "actor --help missing `{subcommand}`: {stdout}"
        );
    }
    let _ = fs::remove_dir_all(&home);
}

#[test]
fn update_help_documents_metadata_replacement() {
    let home = temp_home();
    let args = ["actor", "update", "--help"];
    let output = run(&home, &args);
    let stdout = String::from_utf8_lossy(&output.stdout).into_owned();
    assert!(output.status.success(), "actor update --help failed");
    assert!(
        stdout.contains("REPLACES"),
        "replace-not-merge semantics must be documented: {stdout}"
    );
    let _ = fs::remove_dir_all(&home);
}

#[test]
fn list_help_documents_workspace_scoped_shape() {
    let home = temp_home();
    let args = ["actor", "list", "--help"];
    let output = run(&home, &args);
    let stdout = String::from_utf8_lossy(&output.stdout).into_owned();
    assert!(output.status.success(), "actor list --help failed");
    assert!(
        stdout.contains("bindings"),
        "workspace-scoped output shape must be documented: {stdout}"
    );
    let _ = fs::remove_dir_all(&home);
}
