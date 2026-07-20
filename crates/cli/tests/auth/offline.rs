//! Offline `auth` tests (no valid API key required).

use std::fs;
use std::path::Path;

use crate::common::{assert_failure, assert_success, run, temp_home};

fn memorylake_root(home: &Path) -> std::path::PathBuf {
    home.join(".memorylake")
}

#[test]
fn status_without_login_succeeds() {
    let home = temp_home();
    let args = ["auth", "status"];
    let stdout = assert_success(&run(&home, &args), &args);
    assert!(stdout.contains("Active profile: (none)"));
    assert!(stdout.contains("Logged in: no"));
    assert!(!stdout.contains("Credentials: valid"));
    let _ = fs::remove_dir_all(&home);
}

#[test]
fn switch_unknown_profile_fails() {
    let home = temp_home();
    let args = ["auth", "switch", "missing"];
    let err = assert_failure(&run(&home, &args), &args);
    assert!(
        err.contains("unknown profile") || err.contains("resolve credentials"),
        "unexpected error output: {err}"
    );
    let _ = fs::remove_dir_all(&home);
}

#[test]
fn login_without_api_key_fails_without_tty() {
    let home = temp_home();
    // Non-interactive CI/tests have no TTY, so interactive login must fail clearly.
    let args = ["auth", "login"];
    let err = assert_failure(&run(&home, &args), &args);
    assert!(
        err.contains("TTY") || err.contains("--api-key") || err.contains("interactive"),
        "unexpected error output: {err}"
    );
    let _ = fs::remove_dir_all(&home);
}

#[test]
fn login_with_unreachable_base_url_does_not_persist() {
    let home = temp_home();
    let args = [
        "auth",
        "login",
        "--api-key",
        "sk_bogus_key_offline",
        "--profile",
        "default",
        "--base-url",
        "https://127.0.0.1:1/openapi/memorylake",
    ];
    let err = assert_failure(&run(&home, &args), &args);
    assert!(
        err.contains("could not verify API key")
            || err.contains("could not connect")
            || err.contains("request to MemoryLake API failed"),
        "unexpected error output: {err}"
    );

    let root = memorylake_root(&home);
    assert!(
        !root.join("credentials.toml").is_file(),
        "credentials.toml should not be written after failed login"
    );
    assert!(
        !root.join("config.toml").is_file(),
        "config.toml should not be written after failed login"
    );

    let _ = fs::remove_dir_all(&home);
}
