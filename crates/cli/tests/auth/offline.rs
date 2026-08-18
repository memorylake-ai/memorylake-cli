//! Offline `auth` tests (no valid API key required).

use std::fs;
use std::path::Path;

use crate::common::stub::logged_in_home;
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
fn status_reports_login_state_in_its_output_not_its_exit_code() {
    // `scripts/install.sh` and `install.ps1` decide whether to run their guided
    // setup by reading this line, because `auth status` exits 0 either way —
    // answering "not logged in" is a successful query. Renaming or reformatting
    // the line silently turns the installers' check into "always logged in", so
    // the contract is pinned here.
    let home = temp_home();
    let args = ["auth", "status"];
    let output = run(&home, &args);
    assert!(
        output.status.success(),
        "`auth status` must succeed even when nobody is logged in"
    );
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(
        stdout.contains("Logged in: no"),
        "installers grep for this exact text: {stdout}"
    );
    assert!(
        !stdout.contains("Logged in: yes"),
        "the negative state must not contain the affirmative string: {stdout}"
    );
    let _ = fs::remove_dir_all(&home);
}

#[test]
fn status_reports_being_logged_in_before_it_validates_the_credentials() {
    // Stored credentials that cannot be validated (offline, unreachable API)
    // still count as logged in for the installers' purposes: the line is
    // printed before validation, and the command's non-zero exit does not reach
    // the `grep` reading it through a pipe. Without this, an install run on a
    // flaky network would re-prompt someone who is already set up.
    let home = logged_in_home("http://127.0.0.1:1/openapi/memorylake");
    let args = ["auth", "status"];
    let output = run(&home, &args);
    let stdout = String::from_utf8_lossy(&output.stdout);

    assert!(
        stdout.contains("Logged in: yes"),
        "stored credentials read as logged in: {stdout}"
    );
    assert!(
        !output.status.success(),
        "validation against an unreachable API still fails the command"
    );
    assert!(
        !stdout.contains("Credentials: valid"),
        "and it must not claim the credentials were verified: {stdout}"
    );
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

#[test]
fn login_probes_existing_profile_base_url_without_cli_override() {
    let home = temp_home();
    let root = memorylake_root(&home);
    fs::create_dir_all(&root).expect("create .memorylake");
    fs::write(
        root.join("config.toml"),
        r#"
active_profile = "default"

[profiles.default]
base_url = "https://127.0.0.1:1/openapi/memorylake"
"#,
    )
    .expect("write config.toml");

    let args = [
        "auth",
        "login",
        "--api-key",
        "sk_bogus_key_offline",
        "--profile",
        "default",
    ];
    let err = assert_failure(&run(&home, &args), &args);
    assert!(
        err.contains("https://127.0.0.1:1/openapi/memorylake"),
        "login should probe the profile base URL; unexpected error: {err}"
    );
    assert!(
        !root.join("credentials.toml").is_file(),
        "credentials.toml should not be written after failed login"
    );

    let _ = fs::remove_dir_all(&home);
}
