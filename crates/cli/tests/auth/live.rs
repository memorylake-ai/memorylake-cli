//! Live `auth` tests (require `MEMORYLAKE_API_KEY`).

use std::fs;

use crate::common::{assert_failure, assert_success, require_api_key, run, temp_home};

#[test]
fn login_status_switch_refresh_round_trip() {
    let api_key = require_api_key();
    let home = temp_home();

    let args = [
        "auth",
        "login",
        "--api-key",
        api_key.as_str(),
        "--profile",
        "default",
    ];
    let stdout = assert_success(&run(&home, &args), &args);
    assert!(stdout.contains("Logged in to profile `default`"));

    let args = ["auth", "status"];
    let stdout = assert_success(&run(&home, &args), &args);
    assert!(stdout.contains("Active profile: default"));
    assert!(stdout.contains("Logged in: yes"));
    assert!(stdout.contains("API key source: profile"));
    assert!(stdout.contains("Base URL source:"));
    assert!(stdout.contains("Credentials: valid"));

    let args = [
        "auth",
        "login",
        "--api-key",
        api_key.as_str(),
        "--profile",
        "dev",
    ];
    assert_success(&run(&home, &args), &args);

    let args = ["auth", "switch", "default"];
    let stdout = assert_success(&run(&home, &args), &args);
    assert!(stdout.contains("Switched active profile to `default`"));

    let args = ["auth", "refresh"];
    let stdout = assert_success(&run(&home, &args), &args);
    assert!(stdout.contains("Credentials for profile `default` are valid"));

    let args = ["auth", "logout", "--profile", "default"];
    let stdout = assert_success(&run(&home, &args), &args);
    assert!(stdout.contains("Logged out profile `default`"));

    let _ = fs::remove_dir_all(&home);
}

#[test]
fn login_with_garbage_key_fails_and_does_not_persist() {
    let _api_key = require_api_key();
    let home = temp_home();

    let args = [
        "auth",
        "login",
        "--api-key",
        "sk_definitely_not_a_valid_key_0000",
        "--profile",
        "default",
    ];
    let err = assert_failure(&run(&home, &args), &args);
    assert!(
        err.contains("API key was rejected")
            || err.contains("could not verify API key")
            || err.contains("HTTP 401"),
        "unexpected error output: {err}"
    );

    let root = home.join(".memorylake");
    assert!(
        !root.join("credentials.toml").is_file(),
        "credentials.toml should not be written after failed login"
    );

    let _ = fs::remove_dir_all(&home);
}
