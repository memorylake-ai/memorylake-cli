//! Live `agent` tests (require `MEMORYLAKE_API_KEY`).
//!
//! The lifecycle test creates a real agent and deletes it again, including when
//! an intermediate assertion fails, so a failed run does not leave agents
//! behind on the account.

use std::fs;
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

use serde_json::Value;

use crate::common::{
    assert_failure, assert_success, live_base_url, login_args, require_api_key, run, temp_home,
};

fn login_default(home: &Path, api_key: &str) {
    let base_url = live_base_url();
    let args = login_args(api_key, "default", base_url.as_deref());
    assert_success(&run(home, &args), &args);
}

fn unique_suffix() -> u128 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("clock")
        .as_nanos()
}

fn parse_json(stdout: &str, what: &str) -> Value {
    serde_json::from_str(stdout).unwrap_or_else(|err| panic!("parse {what} JSON: {err}\n{stdout}"))
}

fn str_field<'a>(value: &'a Value, key: &str, what: &str) -> &'a str {
    value
        .get(key)
        .and_then(Value::as_str)
        .unwrap_or_else(|| panic!("{what} response has no string `{key}`: {value}"))
}

/// Deletes the agent when the test ends, including on an assertion panic.
struct AgentCleanup {
    home: PathBuf,
    id: String,
    armed: bool,
}

impl AgentCleanup {
    fn new(home: &Path, id: &str) -> Self {
        Self {
            home: home.to_path_buf(),
            id: id.to_string(),
            armed: true,
        }
    }

    /// Stop cleaning up — the test deleted the agent itself.
    fn disarm(&mut self) {
        self.armed = false;
    }
}

impl Drop for AgentCleanup {
    fn drop(&mut self) {
        if !self.armed {
            return;
        }
        let args = ["agent", "delete", self.id.as_str()];
        let output = run(&self.home, &args);
        if output.status.success() {
            return;
        }
        eprintln!(
            "cleanup: `memorylake agent delete {}` failed:\n{}",
            self.id,
            String::from_utf8_lossy(&output.stderr)
        );
        // Panicking during an unwind aborts the process and would bury the
        // assertion that actually failed, so only escalate on a clean exit.
        if !std::thread::panicking() {
            panic!("cleanup failed to delete agent `{}`", self.id);
        }
    }
}

#[test]
fn full_lifecycle() {
    let api_key = require_api_key();
    let home = temp_home();
    login_default(&home, &api_key);

    let suffix = unique_suffix();
    let custom_id = format!("cli-agent-live-{suffix}");
    let name = format!("CLI Agent Live {suffix}");

    // 1. Create.
    let create_args = [
        "agent",
        "create",
        "--name",
        name.as_str(),
        "--custom-id",
        custom_id.as_str(),
        "--description",
        "created by memorylake-cli agent live test",
    ];
    let created = parse_json(
        &assert_success(&run(&home, &create_args), &create_args),
        "create",
    );
    let id = str_field(&created, "id", "create").to_string();
    let mut cleanup = AgentCleanup::new(&home, &id);
    assert_eq!(str_field(&created, "custom_id", "create"), custom_id);
    assert_eq!(str_field(&created, "name", "create"), name);

    // 2. Get by id and by custom_id resolve to the same agent.
    let by_id_args = ["agent", "get", id.as_str()];
    let by_id = parse_json(
        &assert_success(&run(&home, &by_id_args), &by_id_args),
        "get",
    );
    assert_eq!(str_field(&by_id, "id", "get"), id);

    let by_custom_args = ["agent", "get", custom_id.as_str(), "--by-custom-id"];
    let by_custom = parse_json(
        &assert_success(&run(&home, &by_custom_args), &by_custom_args),
        "get --by-custom-id",
    );
    assert_eq!(
        str_field(&by_custom, "id", "get --by-custom-id"),
        id,
        "custom_id lookup must resolve to the same agent"
    );

    // 3. Update identity in place.
    let renamed = format!("{name} Renamed");
    let update_args = ["agent", "update", id.as_str(), "--name", renamed.as_str()];
    let updated = parse_json(
        &assert_success(&run(&home, &update_args), &update_args),
        "update",
    );
    assert_eq!(str_field(&updated, "name", "update"), renamed);

    // 4. Create a version carrying nested configuration.
    let base_config = home.join("base-version.json");
    fs::write(
        &base_config,
        r#"{"system_prompt":"base prompt","policies":{"max_turns":3}}"#,
    )
    .expect("write base version config");
    let base_config = base_config.to_string_lossy().into_owned();
    let version_args = [
        "agent",
        "version",
        "create",
        id.as_str(),
        "--config",
        base_config.as_str(),
    ];
    let version = parse_json(
        &assert_success(&run(&home, &version_args), &version_args),
        "version create",
    );
    let version_number = version
        .get("version")
        .and_then(Value::as_u64)
        .unwrap_or_else(|| panic!("version create response has no `version`: {version}"));

    // 5. The new version is listed and individually retrievable.
    let list_args = ["agent", "version", "list", id.as_str()];
    let versions = parse_json(
        &assert_success(&run(&home, &list_args), &list_args),
        "version list",
    );
    let listed = versions
        .get("items")
        .and_then(Value::as_array)
        .unwrap_or_else(|| panic!("version list has no `items`: {versions}"));
    assert!(
        listed
            .iter()
            .any(|item| item.get("version").and_then(Value::as_u64) == Some(version_number)),
        "version {version_number} missing from list: {versions}"
    );

    let version_str = version_number.to_string();
    let get_version_args = ["agent", "version", "get", id.as_str(), version_str.as_str()];
    let fetched = parse_json(
        &assert_success(&run(&home, &get_version_args), &get_version_args),
        "version get",
    );
    assert_eq!(
        fetched.get("version").and_then(Value::as_u64),
        Some(version_number)
    );

    // 6. `--from-version latest` keeps base config for keys not overridden.
    let derived_args = [
        "agent",
        "version",
        "create",
        id.as_str(),
        "--from-version",
        "latest",
        "--model",
        "claude-sonnet-4-20250514",
    ];
    let derived = parse_json(
        &assert_success(&run(&home, &derived_args), &derived_args),
        "version create --from-version",
    );
    assert_eq!(
        derived.get("model").and_then(Value::as_str),
        Some("claude-sonnet-4-20250514"),
        "override did not apply: {derived}"
    );
    assert_eq!(
        derived.get("system_prompt").and_then(Value::as_str),
        Some("base prompt"),
        "base config was not carried over: {derived}"
    );
    assert_eq!(
        derived
            .pointer("/policies/max_turns")
            .and_then(Value::as_u64),
        Some(3),
        "nested base config was not carried over: {derived}"
    );

    // 7. Bind to a workspace created for this test, then list the binding.
    let workspace_custom_id = format!("cli-agent-live-ws-{suffix}");
    let workspace_args = [
        "ws",
        "create",
        "--name",
        workspace_custom_id.as_str(),
        "--custom-id",
        workspace_custom_id.as_str(),
    ];
    let workspace = parse_json(
        &assert_success(&run(&home, &workspace_args), &workspace_args),
        "workspace create",
    );
    let workspace_id = str_field(&workspace, "id", "workspace create").to_string();

    let bind_args = [
        "agent",
        "bind",
        id.as_str(),
        "--workspace",
        workspace_id.as_str(),
    ];
    let binding = parse_json(&assert_success(&run(&home, &bind_args), &bind_args), "bind");
    assert_eq!(str_field(&binding, "agent_id", "bind"), id);

    let bindings_args = ["agent", "bindings", "--workspace", workspace_id.as_str()];
    let bound = parse_json(
        &assert_success(&run(&home, &bindings_args), &bindings_args),
        "bindings",
    );
    assert!(
        bound_agent_ids(&bound).contains(&id),
        "agent {id} missing from workspace bindings: {bound}"
    );

    // 8. Unbind removes the binding but keeps the agent.
    let unbind_args = [
        "agent",
        "unbind",
        id.as_str(),
        "--workspace",
        workspace_id.as_str(),
    ];
    let stdout = assert_success(&run(&home, &unbind_args), &unbind_args);
    assert!(
        stdout.contains("Unbound"),
        "unbind should confirm in plain text, got: {stdout}"
    );

    let bound = parse_json(
        &assert_success(&run(&home, &bindings_args), &bindings_args),
        "bindings after unbind",
    );
    assert!(
        !bound_agent_ids(&bound).contains(&id),
        "agent {id} still bound after unbind: {bound}"
    );
    let still_there = ["agent", "get", id.as_str()];
    assert_success(&run(&home, &still_there), &still_there);

    // 9. Delete, then confirm the agent is gone.
    let delete_args = ["agent", "delete", id.as_str()];
    let stdout = assert_success(&run(&home, &delete_args), &delete_args);
    assert!(
        stdout.contains("Deleted agent"),
        "delete should confirm in plain text, got: {stdout}"
    );
    cleanup.disarm();

    let gone_args = ["agent", "get", id.as_str()];
    assert_failure(&run(&home, &gone_args), &gone_args);

    let _ = fs::remove_dir_all(&home);
}

#[test]
fn list_returns_a_page() {
    let api_key = require_api_key();
    let home = temp_home();
    login_default(&home, &api_key);

    let args = ["agent", "list", "--page-size", "1"];
    let stdout = assert_success(&run(&home, &args), &args);
    assert!(
        stdout.contains("\"items\""),
        "list JSON missing items: {stdout}"
    );

    let _ = fs::remove_dir_all(&home);
}

fn bound_agent_ids(bindings: &Value) -> Vec<String> {
    bindings
        .get("items")
        .and_then(Value::as_array)
        .map(|items| {
            items
                .iter()
                .filter_map(|item| item.get("agent_id").and_then(Value::as_str))
                .map(str::to_string)
                .collect()
        })
        .unwrap_or_default()
}
