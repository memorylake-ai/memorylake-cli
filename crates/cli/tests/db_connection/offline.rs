//! Offline `db-connection` tests: help text and the local checks that must
//! fail before any credential is resolved or request sent.

use std::fs;

use crate::common::stub::logged_in_home;
use crate::common::{assert_failure, assert_success, run, run_with_input, temp_home};

/// Nothing listens here, so a command that reaches the network fails with a
/// connection error rather than the local message under test.
const UNREACHABLE: &str = "http://127.0.0.1:9";

#[test]
fn help_lists_every_subcommand() {
    let home = temp_home();
    let args = ["db-connection", "--help"];
    let stdout = assert_success(&run(&home, &args), &args);
    for subcommand in [
        "list", "create", "get", "update", "delete", "test", "schemas",
    ] {
        assert!(
            stdout.contains(subcommand),
            "missing `{subcommand}`: {stdout}"
        );
    }
    let _ = fs::remove_dir_all(&home);
}

#[test]
fn dbconn_alias_resolves() {
    let home = temp_home();
    let args = ["dbconn", "test", "--help"];
    let stdout = assert_success(&run(&home, &args), &args);
    for flag in ["--password-env", "--password-stdin", "--password-file"] {
        assert!(stdout.contains(flag), "missing {flag}: {stdout}");
    }
    let _ = fs::remove_dir_all(&home);
}

#[test]
fn there_is_no_password_flag() {
    // A password on the command line lands in shell history.
    let home = temp_home();
    let args = [
        "db-connection",
        "test",
        "--host",
        "h",
        "--username",
        "u",
        "--database",
        "d",
        "--password",
        "hunter2",
    ];
    let output = assert_failure(&run(&home, &args), &args);
    assert!(output.contains("unexpected argument"), "{output}");
    let _ = fs::remove_dir_all(&home);
}

#[test]
fn a_missing_password_is_reported_before_credentials() {
    // Not logged in at all: the password problem must still be the one shown.
    let home = temp_home();
    let args = [
        "db-connection",
        "create",
        "--name",
        "n",
        "--host",
        "h",
        "--username",
        "u",
        "--database",
        "d",
    ];
    let output = assert_failure(&run(&home, &args), &args);
    assert!(output.contains("needs a password"), "{output}");
    assert!(output.contains("--password-stdin"), "{output}");
    assert!(!output.contains("not logged in"), "{output}");
    let _ = fs::remove_dir_all(&home);
}

#[test]
fn password_sources_are_mutually_exclusive() {
    let home = temp_home();
    let args = [
        "db-connection",
        "test",
        "--host",
        "h",
        "--username",
        "u",
        "--database",
        "d",
        "--password-env",
        "PGPASSWORD",
        "--password-stdin",
    ];
    let output = assert_failure(&run(&home, &args), &args);
    assert!(output.contains("cannot be used with"), "{output}");
    let _ = fs::remove_dir_all(&home);
}

#[test]
fn an_unset_password_variable_is_named() {
    let home = logged_in_home(UNREACHABLE);
    let args = [
        "db-connection",
        "test",
        "--host",
        "h",
        "--username",
        "u",
        "--database",
        "d",
        "--password-env",
        "MLCLI_TEST_UNSET_DB_PASSWORD",
    ];
    let output = assert_failure(&run_with_input(&home, &args, "", &[]), &args);
    assert!(output.contains("MLCLI_TEST_UNSET_DB_PASSWORD"), "{output}");
    let _ = fs::remove_dir_all(&home);
}

#[test]
fn an_empty_password_from_stdin_is_refused() {
    let home = logged_in_home(UNREACHABLE);
    let args = [
        "db-connection",
        "test",
        "--host",
        "h",
        "--username",
        "u",
        "--database",
        "d",
        "--password-stdin",
    ];
    let output = assert_failure(&run_with_input(&home, &args, "\n", &[]), &args);
    assert!(output.contains("password is empty"), "{output}");
    let _ = fs::remove_dir_all(&home);
}

#[test]
fn changing_a_credential_field_requires_the_password() {
    let home = temp_home();
    let args = [
        "db-connection",
        "update",
        "conn-1",
        "--host",
        "new.example.com",
    ];
    let output = assert_failure(&run(&home, &args), &args);
    assert!(output.contains("needs the password again"), "{output}");
    assert!(!output.contains("not logged in"), "{output}");
    let _ = fs::remove_dir_all(&home);
}

#[test]
fn an_empty_update_is_refused() {
    let home = temp_home();
    let args = ["db-connection", "update", "conn-1"];
    let output = assert_failure(&run(&home, &args), &args);
    assert!(output.contains("nothing to update"), "{output}");
    let _ = fs::remove_dir_all(&home);
}

#[test]
fn out_of_range_values_are_refused_locally() {
    let home = temp_home();
    for (args, needle) in [
        (vec!["db-connection", "list", "--page-size", "101"], "101"),
        (
            vec![
                "db-connection",
                "test",
                "--host",
                "h",
                "--port",
                "0",
                "--username",
                "u",
                "--database",
                "d",
                "--password-env",
                "X",
            ],
            "0",
        ),
        (
            vec![
                "db-connection",
                "create",
                "--name",
                "n",
                "--host",
                "h",
                "--username",
                "u",
                "--database",
                "d",
                "--custom-id",
                "_sys_mine",
                "--password-env",
                "X",
            ],
            "_sys_",
        ),
    ] {
        let output = assert_failure(&run(&home, &args), &args);
        assert!(output.contains("invalid value"), "{args:?}: {output}");
        assert!(output.contains(needle), "{args:?}: {output}");
    }
    let _ = fs::remove_dir_all(&home);
}
