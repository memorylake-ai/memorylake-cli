//! Offline team-management tests (no network; temp `$HOME` only).

use std::fs;

use crate::common::{assert_failure, run, temp_home};

#[test]
fn every_management_command_family_requires_login() {
    // One command per family: they all talk to the API, so none may pretend
    // to work while logged out.
    let home = temp_home();
    for args in [
        ["team", "get"].as_slice(),
        ["api-key", "list"].as_slice(),
        ["member", "list"].as_slice(),
        ["invitation", "list"].as_slice(),
        ["usage"].as_slice(),
    ] {
        let err = assert_failure(&run(&home, args), args);
        assert!(
            err.contains("not logged in") || err.contains("resolve API credentials"),
            "{args:?} should require credentials: {err}"
        );
    }
    let _ = fs::remove_dir_all(&home);
}
