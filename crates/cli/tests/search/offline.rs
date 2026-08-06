//! Offline `search` tests (no network; temp `$HOME` only).

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
fn help_lists_the_documented_flags() {
    let home = temp_home();
    let args = ["search", "--help"];
    let stdout = assert_success(&run(&home, &args), &args);
    for flag in [
        "--workspace",
        "--projects",
        "--actors",
        "--types",
        "--top-k",
    ] {
        assert!(
            stdout.contains(flag),
            "`search --help` missing {flag}: {stdout}"
        );
    }
    assert!(
        stdout.contains("QUERY"),
        "query positional missing: {stdout}"
    );
    let _ = fs::remove_dir_all(&home);
}

#[test]
fn help_offers_no_output_or_pagination_flags() {
    // Pretty JSON is the only output mode, and the endpoint has no pagination.
    let home = temp_home();
    let args = ["search", "--help"];
    let stdout = assert_success(&run(&home, &args), &args);
    for flag in ["--json", "--page-size", "--continuation-token", "--output"] {
        assert!(
            !stdout.contains(flag),
            "`search` should not expose {flag}: {stdout}"
        );
    }
    let _ = fs::remove_dir_all(&home);
}

#[test]
fn top_level_help_lists_search() {
    let home = temp_home();
    let args = ["--help"];
    let stdout = assert_success(&run(&home, &args), &args);
    assert!(stdout.contains("search"), "{stdout}");
    let _ = fs::remove_dir_all(&home);
}

#[test]
fn without_login_it_fails_on_credentials() {
    let home = temp_home();
    let args = ["search", "--workspace", "ws-1", "anything"];
    let err = assert_failure(&run(&home, &args), &args);
    assert!(
        err.contains("not logged in") || err.contains("resolve API credentials"),
        "unexpected error output: {err}"
    );
    let _ = fs::remove_dir_all(&home);
}

#[test]
fn missing_workspace_or_query_is_rejected() {
    let home = temp_home();
    for (args, expected) in [
        (["search", "some query"].as_slice(), "--workspace"),
        (["search", "--workspace", "ws-1"].as_slice(), "QUERY"),
    ] {
        let err = assert_failure(&run(&home, args), args);
        assert!(
            err.contains(expected),
            "expected {expected:?} for {args:?}, got: {err}"
        );
    }
    let _ = fs::remove_dir_all(&home);
}

#[test]
fn empty_query_is_rejected_before_any_request() {
    for query in ["", "   "] {
        assert_rejected_locally(
            &["search", "--workspace", "ws-1", query],
            "must not be empty",
        );
    }
}

#[test]
fn unknown_memory_type_is_rejected_before_any_request() {
    for types in ["Document", "facts", "memo", "document,memo"] {
        assert_rejected_locally(
            &["search", "--workspace", "ws-1", "--types", types, "q"],
            "unknown memory type",
        );
    }
}

#[test]
fn the_memory_type_error_lists_the_accepted_values() {
    let home = logged_in_home(UNREACHABLE);
    let args = ["search", "--workspace", "ws-1", "--types", "memo", "q"];
    let err = assert_failure(&run(&home, &args), &args);
    assert!(err.contains("`memo`"), "should name the bad value: {err}");
    assert!(
        err.contains("document, fact"),
        "should list the accepted values: {err}"
    );
    let _ = fs::remove_dir_all(&home);
}

#[test]
fn empty_list_entries_are_rejected_before_any_request() {
    for (flag, value) in [
        ("--projects", "P1,,P2"),
        ("--projects", "P1,"),
        ("--projects", ","),
        ("--actors", "A1,,A2"),
        ("--types", "document,,fact"),
    ] {
        assert_rejected_locally(
            &["search", "--workspace", "ws-1", flag, value, "q"],
            "empty entry",
        );
    }
}

#[test]
fn an_entirely_empty_list_flag_is_rejected_before_any_request() {
    for flag in ["--projects", "--actors", "--types"] {
        assert_rejected_locally(
            &["search", "--workspace", "ws-1", flag, "", "q"],
            "must not be empty",
        );
    }
}
