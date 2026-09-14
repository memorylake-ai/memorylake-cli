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

/// The account's default workspace and the built-in agent bound to it.
///
/// Every account has both, so the A2A tests need no fixture of their own —
/// and creating one would not do: a freshly created agent has no runtime to
/// answer over A2A until it is configured with a model.
fn default_workspace_and_agent(home: &Path) -> (String, String) {
    let ws_args = ["ws", "list", "--page-size", "100"];
    let workspaces = parse_json(&assert_success(&run(home, &ws_args), &ws_args), "ws list");
    let workspace = workspaces["items"]
        .as_array()
        .expect("items")
        .iter()
        .find(|ws| ws["custom_id"] == "_sys_default_workspace")
        .map(|ws| str_field(ws, "id", "ws list").to_string())
        .expect("account has a default workspace");

    let bound_args = ["agent", "bindings", "--workspace", workspace.as_str()];
    let bound = parse_json(
        &assert_success(&run(home, &bound_args), &bound_args),
        "bindings",
    );
    let agent = bound["items"]
        .as_array()
        .expect("items")
        .iter()
        .find(|b| b["custom_id"] == "_sys_builtin_default_agent")
        .map(|b| str_field(b, "agent_id", "bindings").to_string())
        .expect("default agent is bound to the default workspace");

    (workspace, agent)
}

#[test]
fn a2a_round_trip_against_the_default_agent() {
    let api_key = require_api_key();
    let home = temp_home();
    login_default(&home, &api_key);
    let (workspace, agent) = default_workspace_and_agent(&home);
    let ws = workspace.as_str();
    let agent = agent.as_str();

    // 1. The card names the v1.0 HTTP+JSON binding this CLI speaks.
    let card_args = ["agent", "card", agent, "--workspace", ws];
    let card = parse_json(&assert_success(&run(&home, &card_args), &card_args), "card");
    let interfaces = card["supportedInterfaces"].as_array().expect("interfaces");
    assert!(
        interfaces.iter().any(|i| {
            i["protocolBinding"] == "HTTP+JSON"
                && i["protocolVersion"] == "1.0"
                && i["url"].as_str().is_some_and(|u| u.ends_with("/a2a"))
        }),
        "card advertises no v1.0 HTTP+JSON binding on the unversioned path: {card}"
    );

    // 2. A blocking send prints the reply and names the task on stderr.
    //    `--skip-memory` keeps the probe out of the account's memories.
    let send_args = [
        "agent",
        "send",
        agent,
        "--workspace",
        ws,
        "--text",
        "Reply with exactly the word PONG and nothing else.",
        "--skip-memory",
    ];
    let output = run(&home, &send_args);
    let stdout = assert_success(&output, &send_args);
    assert!(
        stdout.to_uppercase().contains("PONG"),
        "reply should contain PONG: {stdout}"
    );
    let stderr = String::from_utf8_lossy(&output.stderr);
    let task_id = stderr
        .lines()
        .find_map(|line| line.strip_prefix("task "))
        .and_then(|rest| rest.split_whitespace().next())
        .unwrap_or_else(|| panic!("stderr names no task: {stderr}"))
        .to_string();
    assert!(stderr.contains("TASK_STATE_COMPLETED"), "{stderr}");

    // 3. The task can be read back, and it appears in the listing.
    let get_args = [
        "agent",
        "task",
        "get",
        agent,
        task_id.as_str(),
        "--workspace",
        ws,
    ];
    let task = parse_json(
        &assert_success(&run(&home, &get_args), &get_args),
        "task get",
    );
    assert_eq!(str_field(&task, "id", "task get"), task_id);

    let list_args = [
        "agent",
        "task",
        "list",
        agent,
        "--workspace",
        ws,
        "--page-size",
        "1",
    ];
    let page = parse_json(
        &assert_success(&run(&home, &list_args), &list_args),
        "task list",
    );
    assert_eq!(page["tasks"].as_array().map(Vec::len), Some(1), "{page}");

    // 4. Feedback lands in the task's metadata and reads back with the
    //    extension header `task get` sends.
    let feedback_args = [
        "agent",
        "task",
        "feedback",
        agent,
        task_id.as_str(),
        "--workspace",
        ws,
        "--rating",
        "up",
        "--comment",
        "memorylake-cli live test",
    ];
    assert_success(&run(&home, &feedback_args), &feedback_args);
    let task = parse_json(
        &assert_success(&run(&home, &get_args), &get_args),
        "task get after feedback",
    );
    assert_eq!(
        task["metadata"]["task-feedback/v1"]["rating"], "up",
        "rating missing after feedback: {task}"
    );

    // 5. Cancelling a finished task is refused with the A2A reason.
    let cancel_args = [
        "agent",
        "task",
        "cancel",
        agent,
        task_id.as_str(),
        "--workspace",
        ws,
    ];
    let err = assert_failure(&run(&home, &cancel_args), &cancel_args);
    assert!(err.contains("TASK_NOT_CANCELABLE"), "{err}");

    // 6. Streaming prints the reply once, not once per token plus the artifact.
    let stream_args = [
        "agent",
        "send",
        agent,
        "--workspace",
        ws,
        "--text",
        "Reply with exactly the word PONG and nothing else.",
        "--skip-memory",
        "--stream",
    ];
    let stdout = assert_success(&run(&home, &stream_args), &stream_args);
    assert_eq!(
        stdout.to_uppercase().matches("PONG").count(),
        1,
        "streamed reply printed more than once: {stdout:?}"
    );

    let _ = fs::remove_dir_all(&home);
}
