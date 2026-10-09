//! Offline `workspace` tests (no network; temp `$HOME` only).

use std::fs;

use crate::common::{assert_failure, assert_success, run, temp_home};

#[test]
fn list_without_login_fails() {
    let home = temp_home();
    let args = ["workspace", "list"];
    let err = assert_failure(&run(&home, &args), &args);
    assert!(
        err.contains("not logged in") || err.contains("resolve API credentials"),
        "unexpected error output: {err}"
    );
    let _ = fs::remove_dir_all(&home);
}

#[test]
fn get_without_login_fails() {
    let home = temp_home();
    let args = ["workspace", "get", "ws-does-not-matter"];
    let err = assert_failure(&run(&home, &args), &args);
    assert!(
        err.contains("not logged in") || err.contains("resolve API credentials"),
        "unexpected error output: {err}"
    );
    let _ = fs::remove_dir_all(&home);
}

#[test]
fn current_and_clear_work_without_logging_in() {
    // Both only read and write local config. Reporting a missing API key to
    // someone asking "which workspace is set?" would answer a question they did
    // not ask — and `--clear` would leave them unable to undo a bad value while
    // logged out.
    let home = temp_home();

    let args = ["workspace", "current"];
    let stdout = assert_success(&run(&home, &args), &args);
    assert!(
        stdout.contains("no workspace set"),
        "current reports the absence rather than a credentials error: {stdout}"
    );
    assert!(
        stdout.contains("workspace use"),
        "and says how to set one: {stdout}"
    );

    let args = ["workspace", "use", "--clear"];
    let stdout = assert_success(&run(&home, &args), &args);
    assert!(
        stdout.contains("no longer remembers"),
        "clearing succeeds while logged out: {stdout}"
    );

    let _ = fs::remove_dir_all(&home);
}

#[test]
fn use_requires_login_because_it_talks_to_the_api() {
    // The other half of the split: choosing a workspace lists them, and naming
    // one verifies it exists, so both need credentials.
    let home = temp_home();
    for args in [
        ["workspace", "use"].as_slice(),
        ["workspace", "use", "ws-1"].as_slice(),
    ] {
        let err = assert_failure(&run(&home, args), args);
        assert!(
            err.contains("not logged in") || err.contains("resolve API credentials"),
            "{args:?} should require credentials: {err}"
        );
    }
    let _ = fs::remove_dir_all(&home);
}

#[test]
fn update_help_documents_replacement_metadata() {
    let home = temp_home();
    let args = ["workspace", "update", "--help"];
    let stdout = assert_success(&run(&home, &args), &args);
    for needle in ["--name", "--description", "--metadata", "replaces"] {
        assert!(stdout.contains(needle), "missing `{needle}`: {stdout}");
    }
    let _ = fs::remove_dir_all(&home);
}

#[test]
fn update_is_rejected_locally_before_credentials_are_needed() {
    // Not logged in: each of these must fail on its own merits, never with
    // "not logged in", which would mean the check came too late.
    let home = temp_home();
    for (args, expected) in [
        (vec!["workspace", "update", "ws-1"], "nothing to update"),
        (
            vec!["workspace", "update", "ws-1", "--name", ""],
            "1 to 255 characters",
        ),
        (
            vec!["workspace", "update", "ws-1", "--metadata", "[1]"],
            "JSON object",
        ),
        (
            vec!["workspace", "update", "ws-1", "--metadata", r#"{"n":1}"#],
            "must be a JSON string",
        ),
    ] {
        let err = assert_failure(&run(&home, &args), &args);
        assert!(err.contains(expected), "{args:?}: {err}");
        assert!(!err.contains("not logged in"), "{args:?}: {err}");
    }

    let long = "x".repeat(2001);
    let args = [
        "workspace",
        "update",
        "ws-1",
        "--description",
        long.as_str(),
    ];
    let err = assert_failure(&run(&home, &args), &args);
    assert!(err.contains("at most 2000 characters"), "{err}");
    let _ = fs::remove_dir_all(&home);
}

#[test]
fn there_is_no_workspace_delete() {
    // Deliberately not exposed: a workspace holds everything else.
    let home = temp_home();
    let args = ["workspace", "delete", "ws-1"];
    let err = assert_failure(&run(&home, &args), &args);
    assert!(err.contains("unrecognized subcommand"), "{err}");
    let _ = fs::remove_dir_all(&home);
}
