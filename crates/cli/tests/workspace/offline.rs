//! Offline `workspace` tests (no network; temp `$HOME` only).

use std::fs;

use crate::common::{assert_failure, run, temp_home};

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
