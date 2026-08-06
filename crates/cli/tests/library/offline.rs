//! Offline `library` tests (no network; temp `$HOME` only).

use std::fs;

use crate::common::{assert_failure, assert_success, run, temp_home};

#[test]
fn get_without_login_fails() {
    let home = temp_home();
    let args = ["library", "get", "MY_SPACE"];
    let err = assert_failure(&run(&home, &args), &args);
    assert!(
        err.contains("not logged in") || err.contains("resolve API credentials"),
        "unexpected error output: {err}"
    );
    let _ = fs::remove_dir_all(&home);
}

#[test]
fn list_without_login_fails() {
    let home = temp_home();
    let args = ["lib", "list"];
    let err = assert_failure(&run(&home, &args), &args);
    assert!(
        err.contains("not logged in") || err.contains("resolve API credentials"),
        "unexpected error output: {err}"
    );
    let _ = fs::remove_dir_all(&home);
}

#[test]
fn delete_without_login_fails() {
    let home = temp_home();
    // Credentials are resolved before anything destructive is attempted.
    let args = ["library", "delete", "sc-a:inode-b"];
    let err = assert_failure(&run(&home, &args), &args);
    assert!(
        err.contains("not logged in") || err.contains("resolve API credentials"),
        "unexpected error output: {err}"
    );
    let _ = fs::remove_dir_all(&home);
}

#[test]
fn help_lists_the_library_subcommands() {
    let home = temp_home();
    let args = ["library", "--help"];
    let stdout = assert_success(&run(&home, &args), &args);
    for subcommand in ["get", "list", "mkdir", "upload", "delete"] {
        assert!(
            stdout.contains(subcommand),
            "`library --help` should mention `{subcommand}`: {stdout}"
        );
    }
    let _ = fs::remove_dir_all(&home);
}

#[test]
fn on_conflict_rejects_an_unknown_strategy() {
    let home = temp_home();
    // clap validates the value set before any credential resolution happens.
    let args = ["library", "mkdir", "docs", "--on-conflict", "clobber"];
    let err = assert_failure(&run(&home, &args), &args);
    assert!(
        err.contains("clobber") && err.contains("rename"),
        "unexpected error output: {err}"
    );
    let _ = fs::remove_dir_all(&home);
}
