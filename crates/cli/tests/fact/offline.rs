//! Offline `fact` tests (no network; temp `$HOME` only).

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
fn help_lists_fact_subcommands() {
    let home = temp_home();
    let args = ["fact", "--help"];
    let stdout = assert_success(&run(&home, &args), &args);
    for subcommand in ["add", "delete", "list"] {
        assert!(
            stdout.contains(subcommand),
            "`fact --help` missing {subcommand}: {stdout}"
        );
    }
    let _ = fs::remove_dir_all(&home);
}

#[test]
fn add_requires_exactly_one_scope() {
    assert_rejected_locally(&["fact", "add", "--workspace", "ws-1", "text"], "--actor");
    assert_rejected_locally(
        &[
            "fact",
            "add",
            "--workspace",
            "ws-1",
            "--actor",
            "actor-1",
            "--project",
            "proj-1",
            "text",
        ],
        "not both",
    );
}

#[test]
fn add_requires_at_least_one_fact() {
    let home = temp_home();
    let args = ["fact", "add", "--workspace", "ws-1", "--actor", "actor-1"];
    let err = assert_failure(&run(&home, &args), &args);
    assert!(err.contains("TEXT"), "missing-text error: {err}");
    let _ = fs::remove_dir_all(&home);
}

#[test]
fn delete_requires_exactly_one_scope() {
    assert_rejected_locally(
        &["fact", "delete", "--workspace", "ws-1", "fact-1"],
        "--project",
    );
    assert_rejected_locally(
        &[
            "fact",
            "delete",
            "--workspace",
            "ws-1",
            "--actor",
            "actor-1",
            "--project",
            "proj-1",
            "fact-1",
        ],
        "not both",
    );
}

#[test]
fn delete_requires_at_least_one_fact_id() {
    let home = temp_home();
    let args = [
        "fact",
        "delete",
        "--workspace",
        "ws-1",
        "--actor",
        "actor-1",
    ];
    let err = assert_failure(&run(&home, &args), &args);
    assert!(err.contains("FACT_ID"), "missing-id error: {err}");
    let _ = fs::remove_dir_all(&home);
}

#[test]
fn list_requires_at_least_one_filter() {
    // With neither filter the API answers an empty page rather than every
    // fact, so an unfiltered list must be rejected before any request.
    assert_rejected_locally(
        &["fact", "list", "--workspace", "ws-1"],
        "at least one of --actors / --projects",
    );
}

#[test]
fn list_rejects_empty_filter_entries() {
    assert_rejected_locally(
        &["fact", "list", "--workspace", "ws-1", "--actors", "a,,b"],
        "empty entry",
    );
}

#[test]
fn every_subcommand_without_login_fails() {
    let home = temp_home();
    for args in [
        vec![
            "fact",
            "add",
            "--workspace",
            "ws-1",
            "--actor",
            "actor-1",
            "t",
        ],
        vec![
            "fact",
            "delete",
            "--workspace",
            "ws-1",
            "--actor",
            "actor-1",
            "fact-1",
        ],
        vec!["fact", "list", "--workspace", "ws-1", "--actors", "actor-1"],
    ] {
        let err = assert_failure(&run(&home, &args), &args);
        assert!(
            err.contains("not logged in"),
            "expected login error for {args:?}: {err}"
        );
    }
    let _ = fs::remove_dir_all(&home);
}
