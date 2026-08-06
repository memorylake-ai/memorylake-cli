//! Offline `agent` tests (no network; temp `$HOME` only).
//!
//! Several cases need a *resolvable* profile so the command gets past
//! credential resolution and reaches request-body construction. Those seed
//! `~/.memorylake` directly and point the profile at an unreachable base URL,
//! so any request that did escape would surface as a connection error — which
//! the assertions explicitly rule out.

use std::fs;
use std::path::{Path, PathBuf};

use crate::common::{assert_failure, assert_success, run, temp_home};

/// Base URL that refuses connections immediately.
const UNREACHABLE_BASE_URL: &str = "https://127.0.0.1:1/openapi/memorylake";

/// Seed a resolvable profile without contacting the API.
fn seed_credentials(home: &Path) {
    let root = home.join(".memorylake");
    fs::create_dir_all(&root).expect("create .memorylake");
    fs::write(
        root.join("config.toml"),
        format!("active_profile = \"default\"\n\n[profiles.default]\nbase_url = \"{UNREACHABLE_BASE_URL}\"\n"),
    )
    .expect("write config.toml");
    fs::write(
        root.join("credentials.toml"),
        "[profiles.default]\napi_key = \"sk_offline_test_key\"\nlogin_method = \"api_key\"\n",
    )
    .expect("write credentials.toml");
}

fn write_config(home: &Path, name: &str, contents: &str) -> PathBuf {
    let path = home.join(name);
    fs::write(&path, contents).expect("write agent config file");
    path
}

/// Assert the command failed locally rather than by reaching the network.
fn assert_no_request_attempted(err: &str) {
    for marker in [
        "could not connect",
        "request to MemoryLake API failed",
        "timed out connecting",
    ] {
        assert!(
            !err.contains(marker),
            "command reached the network before failing locally: {err}"
        );
    }
}

#[test]
fn help_lists_every_agent_subcommand() {
    let home = temp_home();
    let args = ["agent", "--help"];
    let stdout = assert_success(&run(&home, &args), &args);
    for subcommand in [
        "list", "create", "get", "update", "delete", "version", "bind", "unbind", "bindings",
    ] {
        assert!(
            stdout.contains(subcommand),
            "`agent --help` is missing `{subcommand}`: {stdout}"
        );
    }
    let _ = fs::remove_dir_all(&home);
}

#[test]
fn create_help_lists_the_documented_flags() {
    let home = temp_home();
    let args = ["agent", "create", "--help"];
    let stdout = assert_success(&run(&home, &args), &args);
    for flag in [
        "--name",
        "--custom-id",
        "--description",
        "--model",
        "--system-prompt",
        "--config",
    ] {
        assert!(
            stdout.contains(flag),
            "`agent create --help` is missing `{flag}`: {stdout}"
        );
    }
    let _ = fs::remove_dir_all(&home);
}

#[test]
fn delete_help_states_the_operation_is_irreversible() {
    let home = temp_home();
    let args = ["agent", "delete", "--help"];
    let stdout = assert_success(&run(&home, &args), &args);
    assert!(
        stdout.contains("Irreversible"),
        "`agent delete --help` must warn about irreversibility: {stdout}"
    );
    assert!(
        stdout.contains("no confirmation prompt"),
        "`agent delete --help` must say there is no prompt: {stdout}"
    );
    let _ = fs::remove_dir_all(&home);
}

#[test]
fn list_without_login_fails() {
    let home = temp_home();
    let args = ["agent", "list"];
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
    let args = ["agent", "get", "agt-does-not-matter"];
    let err = assert_failure(&run(&home, &args), &args);
    assert!(
        err.contains("not logged in") || err.contains("resolve API credentials"),
        "unexpected error output: {err}"
    );
    let _ = fs::remove_dir_all(&home);
}

#[test]
fn version_and_binding_subcommands_without_login_fail() {
    let home = temp_home();
    for args in [
        vec!["agent", "version", "list", "agt-1"],
        vec!["agent", "version", "get", "agt-1", "1"],
        vec!["agent", "bindings", "--workspace", "ws-1"],
        vec!["agent", "bind", "agt-1", "--workspace", "ws-1"],
        vec!["agent", "unbind", "agt-1", "--workspace", "ws-1"],
        vec!["agent", "delete", "agt-1"],
    ] {
        let err = assert_failure(&run(&home, &args), &args);
        assert!(
            err.contains("not logged in") || err.contains("resolve API credentials"),
            "unexpected error output for {args:?}: {err}"
        );
    }
    let _ = fs::remove_dir_all(&home);
}

#[test]
fn create_without_required_fields_fails() {
    let home = temp_home();
    seed_credentials(&home);

    let args = ["agent", "create", "--name", "Only A Name"];
    let err = assert_failure(&run(&home, &args), &args);
    assert!(err.contains("custom_id"), "unexpected error output: {err}");
    assert!(
        err.contains("--custom-id"),
        "unexpected error output: {err}"
    );
    assert_no_request_attempted(&err);

    let args = ["agent", "create", "--custom-id", "only-an-id"];
    let err = assert_failure(&run(&home, &args), &args);
    assert!(
        err.contains("`name` is required"),
        "unexpected error: {err}"
    );
    assert_no_request_attempted(&err);

    let _ = fs::remove_dir_all(&home);
}

#[test]
fn create_accepts_required_fields_from_the_config_file() {
    let home = temp_home();
    seed_credentials(&home);
    let config = write_config(
        &home,
        "agent.json",
        r#"{"name":"From File","custom_id":"from-file-1"}"#,
    );

    // Required-field validation is satisfied by the file, so the command gets
    // as far as the (unreachable) API instead of failing locally.
    let config = config.to_string_lossy().into_owned();
    let args = ["agent", "create", "--config", config.as_str()];
    let err = assert_failure(&run(&home, &args), &args);
    assert!(
        err.contains("could not connect") || err.contains("request to MemoryLake API failed"),
        "expected a transport error once the body validated, got: {err}"
    );

    let _ = fs::remove_dir_all(&home);
}

#[test]
fn create_with_missing_config_file_fails() {
    let home = temp_home();
    seed_credentials(&home);

    let missing = home.join("nope.json").to_string_lossy().into_owned();
    let args = ["agent", "create", "--config", missing.as_str()];
    let err = assert_failure(&run(&home, &args), &args);
    assert!(err.contains("read config file"), "unexpected error: {err}");
    assert_no_request_attempted(&err);

    let _ = fs::remove_dir_all(&home);
}

#[test]
fn create_with_malformed_config_json_fails() {
    let home = temp_home();
    seed_credentials(&home);
    let config = write_config(&home, "broken.json", "{not json")
        .to_string_lossy()
        .into_owned();

    let args = ["agent", "create", "--config", config.as_str()];
    let err = assert_failure(&run(&home, &args), &args);
    assert!(
        err.contains("parse JSON from config file"),
        "unexpected error: {err}"
    );
    assert_no_request_attempted(&err);

    let _ = fs::remove_dir_all(&home);
}

#[test]
fn create_with_non_object_config_top_level_fails() {
    let home = temp_home();
    seed_credentials(&home);
    let config = write_config(&home, "array.json", r#"["a","b"]"#)
        .to_string_lossy()
        .into_owned();

    let args = ["agent", "create", "--config", config.as_str()];
    let err = assert_failure(&run(&home, &args), &args);
    assert!(
        err.contains("must hold a JSON object"),
        "unexpected error: {err}"
    );
    assert!(err.contains("an array"), "unexpected error: {err}");
    assert_no_request_attempted(&err);

    let _ = fs::remove_dir_all(&home);
}

#[test]
fn update_rejects_config_fields_from_flags_and_file() {
    let home = temp_home();
    seed_credentials(&home);
    let config = write_config(
        &home,
        "with-model.json",
        r#"{"name":"Renamed","model":"gpt-4o","policies":{"max_turns":2}}"#,
    )
    .to_string_lossy()
    .into_owned();

    let args = ["agent", "update", "agt-1", "--config", config.as_str()];
    let err = assert_failure(&run(&home, &args), &args);
    assert!(err.contains("model"), "offending key not named: {err}");
    assert!(err.contains("policies"), "offending key not named: {err}");
    assert!(
        err.contains("agent version create"),
        "error must point at the version command: {err}"
    );
    assert_no_request_attempted(&err);

    let _ = fs::remove_dir_all(&home);
}

#[test]
fn update_allows_identity_fields() {
    let home = temp_home();
    seed_credentials(&home);
    let config = write_config(&home, "identity.json", r#"{"metadata":{"team":"core"}}"#)
        .to_string_lossy()
        .into_owned();

    let args = [
        "agent",
        "update",
        "agt-1",
        "--name",
        "Renamed",
        "--config",
        config.as_str(),
    ];
    let err = assert_failure(&run(&home, &args), &args);
    assert!(
        err.contains("could not connect") || err.contains("request to MemoryLake API failed"),
        "identity-only update must pass the guardrail and attempt the request, got: {err}"
    );

    let _ = fs::remove_dir_all(&home);
}

#[test]
fn update_without_any_field_fails() {
    let home = temp_home();
    seed_credentials(&home);

    let args = ["agent", "update", "agt-1"];
    let err = assert_failure(&run(&home, &args), &args);
    assert!(err.contains("nothing to update"), "unexpected error: {err}");
    assert_no_request_attempted(&err);

    let _ = fs::remove_dir_all(&home);
}

#[test]
fn version_create_rejects_a_bad_from_version() {
    let home = temp_home();
    seed_credentials(&home);

    let args = [
        "agent",
        "version",
        "create",
        "agt-1",
        "--from-version",
        "v3",
    ];
    let err = assert_failure(&run(&home, &args), &args);
    assert!(
        err.contains("expected `latest` or a version number"),
        "unexpected error: {err}"
    );
    assert_no_request_attempted(&err);

    let _ = fs::remove_dir_all(&home);
}

#[test]
fn version_get_rejects_a_non_numeric_version() {
    let home = temp_home();
    seed_credentials(&home);

    let args = ["agent", "version", "get", "agt-1", "latest"];
    let err = assert_failure(&run(&home, &args), &args);
    assert_no_request_attempted(&err);

    let _ = fs::remove_dir_all(&home);
}

#[test]
fn bind_and_bindings_require_a_workspace() {
    let home = temp_home();
    seed_credentials(&home);

    for args in [
        vec!["agent", "bind", "agt-1"],
        vec!["agent", "unbind", "agt-1"],
        vec!["agent", "bindings"],
    ] {
        let err = assert_failure(&run(&home, &args), &args);
        assert!(
            err.contains("--workspace"),
            "unexpected error for {args:?}: {err}"
        );
        assert_no_request_attempted(&err);
    }

    let _ = fs::remove_dir_all(&home);
}
