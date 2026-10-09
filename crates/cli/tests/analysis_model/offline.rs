//! Offline `analysis-model` tests (no network; temp `$HOME` only).
//!
//! Local input errors must win over the not-logged-in error, otherwise they
//! are only reachable for authenticated users.

use std::fs;

use crate::common::{assert_failure, assert_success, run, temp_home, unique_name};

/// Run `args` without credentials and return the combined output, asserting
/// that it failed for `expected` rather than for want of a login.
fn fails_locally(args: &[&str], expected: &str) {
    let home = temp_home();
    let err = assert_failure(&run(&home, args), args);
    let _ = fs::remove_dir_all(&home);
    assert!(err.contains(expected), "expected `{expected}` in: {err}");
    assert!(
        !err.contains("not logged in") && !err.contains("resolve API credentials"),
        "input validation must run before credential resolution: {err}"
    );
}

#[test]
fn help_lists_every_subcommand() {
    let home = temp_home();
    let args = ["analysis-model", "--help"];
    let stdout = assert_success(&run(&home, &args), &args);
    for sub in [
        "templates",
        "list",
        "create",
        "get",
        "update",
        "delete",
        "entry",
    ] {
        assert!(stdout.contains(sub), "missing `{sub}` in help: {stdout}");
    }

    let args = ["am", "entry", "--help"];
    let stdout = assert_success(&run(&home, &args), &args);
    for sub in [
        "list", "get", "create", "update", "delete", "disable", "enable",
    ] {
        assert!(stdout.contains(sub), "missing `{sub}` in help: {stdout}");
    }
    let _ = fs::remove_dir_all(&home);
}

#[test]
fn subcommands_without_login_fail() {
    let home = temp_home();
    let cases: [&[&str]; 4] = [
        &["am", "list", "--workspace", "ws-1"],
        &["am", "get", "--workspace", "ws-1", "am-1"],
        &[
            "am",
            "entry",
            "list",
            "--workspace",
            "ws-1",
            "--model",
            "am-1",
            "--entity-type",
            "few_shot",
        ],
        &[
            "am",
            "entry",
            "disable",
            "--workspace",
            "ws-1",
            "--model",
            "am-1",
            "e-1",
        ],
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
fn model_update_without_any_field_fails_before_credentials() {
    fails_locally(
        &["am", "update", "--workspace", "ws-1", "am-1"],
        "at least one of --name or --description",
    );
}

#[test]
fn entry_update_with_only_a_kind_fails_before_credentials() {
    fails_locally(
        &[
            "am",
            "entry",
            "update",
            "--workspace",
            "ws-1",
            "--model",
            "am-1",
            "e-1",
            "--entity-type",
            "few_shot",
        ],
        "at least one of --embedding, --payload, --payload-file, or --extra",
    );
}

#[test]
fn entry_update_requires_the_kind() {
    fails_locally(
        &[
            "am",
            "entry",
            "update",
            "--model",
            "am-1",
            "e-1",
            "--embedding",
            "x",
        ],
        "--entity-type",
    );
}

#[test]
fn entry_list_requires_the_kind() {
    fails_locally(&["am", "entry", "list", "--model", "am-1"], "--entity-type");
}

#[test]
fn entry_create_requires_a_payload() {
    fails_locally(
        &[
            "am",
            "entry",
            "create",
            "--model",
            "am-1",
            "--entity-type",
            "few_shot",
            "--embedding",
            "q",
        ],
        "--payload",
    );
}

#[test]
fn entry_create_rejects_both_payload_sources() {
    // The file must exist: `--payload-file` is read while arguments are
    // parsed, and a missing file would fail for that reason instead.
    let dir = std::env::temp_dir().join(unique_name("am-both-payloads"));
    fs::create_dir_all(&dir).expect("create scratch dir");
    let path = dir.join("payload.json");
    fs::write(&path, "{}").expect("write payload file");
    let path_arg = path.to_string_lossy().into_owned();

    fails_locally(
        &[
            "am",
            "entry",
            "create",
            "--model",
            "am-1",
            "--entity-type",
            "few_shot",
            "--payload",
            "{}",
            "--payload-file",
            path_arg.as_str(),
        ],
        "cannot be used with",
    );
    let _ = fs::remove_dir_all(&dir);
}

#[test]
fn entry_create_rejects_a_non_object_payload() {
    fails_locally(
        &[
            "am",
            "entry",
            "create",
            "--model",
            "am-1",
            "--entity-type",
            "few_shot",
            "--payload",
            "[1,2]",
        ],
        "must be a JSON object, got an array",
    );
}

#[test]
fn entry_create_rejects_a_missing_payload_file() {
    fails_locally(
        &[
            "am",
            "entry",
            "create",
            "--model",
            "am-1",
            "--entity-type",
            "few_shot",
            "--payload-file",
            "/definitely/not/here.json",
        ],
        "cannot read /definitely/not/here.json",
    );
}

#[test]
fn entry_list_rejects_an_unknown_origin() {
    fails_locally(
        &[
            "am",
            "entry",
            "list",
            "--model",
            "am-1",
            "--entity-type",
            "few_shot",
            "--from",
            "manual",
        ],
        "MANUAL",
    );
}

#[test]
fn page_size_out_of_range_is_rejected() {
    fails_locally(&["am", "list", "--page-size", "101"], "--page-size");
    fails_locally(
        &[
            "am",
            "entry",
            "list",
            "--model",
            "am-1",
            "--entity-type",
            "few_shot",
            "--page-size",
            "0",
        ],
        "--page-size",
    );
}
