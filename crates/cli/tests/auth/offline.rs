//! Offline `auth` tests (no network; temp `$HOME` only).

use std::fs;

use crate::common::{assert_success, run, temp_home};

#[test]
fn login_status_switch_logout_round_trip() {
    let home = temp_home();

    let args = [
        "auth",
        "login",
        "api_key",
        "--api-key",
        "sk_testkey1234",
        "--profile",
        "default",
    ];
    let stdout = assert_success(&run(&home, &args), &args);
    assert!(stdout.contains("Logged in to profile `default`"));

    let args = [
        "auth",
        "login",
        "api_key",
        "--api-key",
        "sk_devkey5678",
        "--profile",
        "dev",
        "--base-url",
        "https://example.test/openapi/memorylake",
    ];
    assert_success(&run(&home, &args), &args);

    let args = ["auth", "status"];
    let stdout = assert_success(&run(&home, &args), &args);
    assert!(stdout.contains("Active profile: dev"));
    assert!(stdout.contains("Logged in: yes"));
    assert!(stdout.contains("Login method: api_key"));
    assert!(stdout.contains("https://example.test/openapi/memorylake"));

    let args = ["auth", "switch", "default"];
    let stdout = assert_success(&run(&home, &args), &args);
    assert!(stdout.contains("Switched active profile to `default`"));

    let args = ["auth", "status"];
    let stdout = assert_success(&run(&home, &args), &args);
    assert!(stdout.contains("Active profile: default"));

    let args = ["auth", "logout", "--profile", "default"];
    let stdout = assert_success(&run(&home, &args), &args);
    assert!(stdout.contains("Logged out profile `default`"));

    let args = ["-v", "auth", "status"];
    let stdout = assert_success(&run(&home, &args), &args);
    assert!(stdout.contains("Active profile:"));

    let _ = fs::remove_dir_all(&home);
}
