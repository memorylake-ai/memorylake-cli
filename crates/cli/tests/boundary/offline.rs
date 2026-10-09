//! Offline `boundary` tests (no network; temp `$HOME` only).

use std::fs;

use crate::common::{assert_failure, assert_success, run, temp_home};

#[test]
fn help_lists_every_boundary_subcommand() {
    let home = temp_home();
    let args = ["boundary", "--help"];
    let stdout = assert_success(&run(&home, &args), &args);
    for subcommand in ["list", "create", "get", "update", "delete"] {
        assert!(
            stdout.contains(subcommand),
            "missing `{subcommand}`: {stdout}"
        );
    }
    let _ = fs::remove_dir_all(&home);
}

#[test]
fn update_help_lists_the_clear_flags_and_the_replace_warning() {
    let home = temp_home();
    let args = ["boundary", "update", "--help"];
    let stdout = assert_success(&run(&home, &args), &args);
    for needle in [
        "--projects",
        "--clear-projects",
        "--human-actor",
        "--clear-human-actor",
        "--agent",
        "--clear-agent",
        "REPLACES",
    ] {
        assert!(stdout.contains(needle), "missing `{needle}`: {stdout}");
    }
    let _ = fs::remove_dir_all(&home);
}

#[test]
fn update_without_changes_is_refused_before_login() {
    let home = temp_home();
    let args = ["boundary", "update", "bnd-1"];
    let err = assert_failure(&run(&home, &args), &args);
    assert!(err.contains("nothing to update"), "{err}");
    assert!(
        !err.contains("not logged in"),
        "the check must run before credentials are resolved: {err}"
    );
    let _ = fs::remove_dir_all(&home);
}

#[test]
fn a_value_and_its_clear_flag_conflict() {
    let home = temp_home();
    for args in [
        vec![
            "boundary",
            "update",
            "bnd-1",
            "--agent",
            "agent-1",
            "--clear-agent",
        ],
        vec![
            "boundary",
            "update",
            "bnd-1",
            "--human-actor",
            "a",
            "--clear-human-actor",
        ],
        vec![
            "boundary",
            "update",
            "bnd-1",
            "--projects",
            "p",
            "--clear-projects",
        ],
    ] {
        let err = assert_failure(&run(&home, &args), &args);
        assert!(err.contains("cannot be used with"), "{args:?}: {err}");
    }
    let _ = fs::remove_dir_all(&home);
}

#[test]
fn invalid_values_are_rejected_by_the_parser() {
    let home = temp_home();
    let long_name = "x".repeat(256);
    for args in [
        vec!["boundary", "create", "--name", ""],
        vec!["boundary", "create", "--name", long_name.as_str()],
        vec!["boundary", "create", "--name", "n", "--projects", "a,,b"],
        vec!["boundary", "update", "bnd-1", "--agent", " "],
        vec!["boundary", "update", "bnd-1", "--human-actor", ""],
        vec!["boundary", "list", "--page-size", "0"],
    ] {
        let err = assert_failure(&run(&home, &args), &args);
        assert!(err.contains("invalid value"), "{args:?}: {err}");
    }
    let _ = fs::remove_dir_all(&home);
}

#[test]
fn create_requires_a_name() {
    let home = temp_home();
    let args = ["boundary", "create", "--workspace", "ws-1"];
    let err = assert_failure(&run(&home, &args), &args);
    assert!(err.contains("--name"), "{err}");
    let _ = fs::remove_dir_all(&home);
}

#[test]
fn network_commands_without_login_fail() {
    let home = temp_home();
    for args in [
        vec!["boundary", "list", "--workspace", "ws-1"],
        vec!["boundary", "get", "bnd-1"],
        vec!["boundary", "delete", "bnd-1"],
        vec!["boundary", "update", "bnd-1", "--name", "n"],
    ] {
        let err = assert_failure(&run(&home, &args), &args);
        assert!(err.contains("not logged in"), "{args:?}: {err}");
    }
    let _ = fs::remove_dir_all(&home);
}
