//! Offline `datasource` tests: help text and local checks that must fail
//! before credentials are resolved.

use std::fs;

use crate::common::stub::logged_in_home;
use crate::common::{assert_failure, assert_success, run, run_with_input, temp_home};

#[test]
fn help_lists_every_subcommand() {
    let home = temp_home();
    let args = ["datasource", "--help"];
    let stdout = assert_success(&run(&home, &args), &args);
    for subcommand in [
        "list", "create", "get", "update", "delete", "build", "tables", "columns", "annotate",
    ] {
        assert!(
            stdout.contains(subcommand),
            "missing `{subcommand}`: {stdout}"
        );
    }
    let _ = fs::remove_dir_all(&home);
}

#[test]
fn ds_alias_resolves_and_create_takes_one_schema() {
    let home = temp_home();
    let args = ["ds", "create", "--help"];
    let stdout = assert_success(&run(&home, &args), &args);
    for flag in [
        "--workspace",
        "--connection",
        "--schema",
        "--name",
        "--custom-id",
        "--table-filter",
    ] {
        assert!(stdout.contains(flag), "missing {flag}: {stdout}");
    }
    let _ = fs::remove_dir_all(&home);
}

#[test]
fn columns_requires_a_table() {
    let home = temp_home();
    let args = ["datasource", "columns", "ds-1"];
    let output = assert_failure(&run(&home, &args), &args);
    assert!(output.contains("--table"), "{output}");
    let _ = fs::remove_dir_all(&home);
}

#[test]
fn an_empty_update_is_refused_before_credentials() {
    let home = temp_home();
    let args = ["datasource", "update", "ds-1"];
    let output = assert_failure(&run(&home, &args), &args);
    assert!(output.contains("nothing to update"), "{output}");
    assert!(!output.contains("not logged in"), "{output}");
    let _ = fs::remove_dir_all(&home);
}

#[test]
fn annotate_needs_something_to_change() {
    let home = temp_home();
    for (args, needle) in [
        (vec!["datasource", "annotate", "ds-1"], "--table"),
        (
            vec!["datasource", "annotate", "ds-1", "--table", "orders"],
            "nothing to change",
        ),
        (
            vec![
                "datasource",
                "annotate",
                "ds-1",
                "--table",
                "orders",
                "--embedding",
                "true",
            ],
            "--column",
        ),
        (
            vec![
                "datasource",
                "annotate",
                "ds-1",
                "--table",
                "orders",
                "--edits",
                "[]",
            ],
            "cannot be used with",
        ),
    ] {
        let output = assert_failure(&run(&home, &args), &args);
        assert!(output.contains(needle), "{args:?}: {output}");
        assert!(!output.contains("not logged in"), "{args:?}: {output}");
    }
    let _ = fs::remove_dir_all(&home);
}

#[test]
fn malformed_edit_batches_are_refused_before_credentials() {
    let home = temp_home();
    for (json, needle) in [
        ("not json", "JSON array of edits"),
        ("[]", "holds no edits"),
        (
            r#"[{"target":"table","table_name":"t","coment":"typo"}]"#,
            "JSON array of edits",
        ),
        (
            r#"[{"target":"column","table_name":"t","column_name":"c"}]"#,
            "edit 0 changes nothing",
        ),
    ] {
        let args = ["datasource", "annotate", "ds-1", "--edits-file", "-"];
        let output = assert_failure(&run_with_input(&home, &args, json, &[]), &args);
        assert!(output.contains(needle), "{json}: {output}");
        assert!(!output.contains("not logged in"), "{json}: {output}");
    }
    let _ = fs::remove_dir_all(&home);
}

#[test]
fn a_workspace_is_required() {
    // Logged in, but no workspace remembered or given.
    let home = logged_in_home("http://127.0.0.1:9");
    let args = ["datasource", "list"];
    let output = assert_failure(&run(&home, &args), &args);
    assert!(output.contains("no workspace given"), "{output}");
    let _ = fs::remove_dir_all(&home);
}
