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
