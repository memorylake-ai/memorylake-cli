//! Live `fact` tests (require `MEMORYLAKE_API_KEY`).
//!
//! The full lifecycle runs against a scratch project rather than an actor:
//! projects can be created and deleted per test, so nothing durable is left
//! behind, while actors are account-wide. The workspace itself is left behind
//! because `workspace delete` is not implemented.
//!
//! `fact_agent_scope_edit_trace_and_instruction` covers the agent scope, the
//! single-fact endpoints, and memory settings against a scratch agent and
//! project, both removed by a [`Scratch`] guard however the test ends. Conflict
//! *resolution* is not exercised live: detection is asynchronous and its
//! outcome is a model's judgment, so a test waiting on it would be slow and
//! flaky. Listing and the error path are covered instead.

use std::fs;
use std::path::{Path, PathBuf};

use serde_json::Value;

use crate::common::{
    assert_success, live_base_url, login_args, require_api_key, run, temp_home, unique_name,
};

fn login_default(home: &Path, api_key: &str) {
    let base_url = live_base_url();
    let args = login_args(api_key, "default", base_url.as_deref());
    assert_success(&run(home, &args), &args);
}

/// Create a workspace and return its id. Left behind — there is no delete.
fn create_workspace(home: &Path) -> String {
    let name = unique_name("fact-ws");
    let args = [
        "workspace",
        "create",
        "--name",
        name.as_str(),
        "--custom-id",
        name.as_str(),
    ];
    let stdout = assert_success(&run(home, &args), &args);
    let created: Value = serde_json::from_str(&stdout).expect("parse workspace create JSON");
    created
        .get("id")
        .and_then(Value::as_str)
        .expect("workspace create response has id")
        .to_string()
}

/// Create a scratch project in `workspace` and return its id.
fn create_project(home: &Path, workspace: &str) -> String {
    let name = unique_name("fact-proj");
    let args = [
        "project",
        "create",
        "--workspace",
        workspace,
        "--name",
        name.as_str(),
        "--custom-id",
        name.as_str(),
    ];
    let stdout = assert_success(&run(home, &args), &args);
    let created: Value = serde_json::from_str(&stdout).expect("parse project create JSON");
    created
        .get("id")
        .and_then(Value::as_str)
        .expect("project create response has id")
        .to_string()
}

fn delete_project(home: &Path, workspace: &str, project: &str) {
    let args = ["project", "delete", "--workspace", workspace, project];
    assert_success(&run(home, &args), &args);
}

#[test]
fn fact_lifecycle_add_list_delete() {
    let api_key = require_api_key();
    let home = temp_home();
    login_default(&home, &api_key);

    let workspace = create_workspace(&home);
    let project = create_project(&home, &workspace);

    // Add two facts in one call; each must come back with an id.
    let args = [
        "fact",
        "add",
        "--workspace",
        workspace.as_str(),
        "--project",
        project.as_str(),
        "cli-fact-live test fact one",
        "cli-fact-live test fact two",
    ];
    let stdout = assert_success(&run(&home, &args), &args);
    let created: Value = serde_json::from_str(&stdout).expect("parse add JSON");
    let created = created
        .get("facts")
        .and_then(Value::as_array)
        .expect("add prints the full payload with a facts array");
    assert_eq!(created.len(), 2, "two facts in, two out: {created:?}");
    let first_id = created[0]
        .get("id")
        .and_then(Value::as_str)
        .expect("created fact has id")
        .to_string();
    assert!(
        first_id.starts_with("fact-"),
        "unexpected fact id shape: {first_id}"
    );

    // Both facts must be visible in the filtered listing, attributed to the
    // project scope they were stored under.
    let args = [
        "fact",
        "list",
        "--workspace",
        workspace.as_str(),
        "--projects",
        project.as_str(),
    ];
    let stdout = assert_success(&run(&home, &args), &args);
    let listing: Value = serde_json::from_str(&stdout).expect("parse list JSON");
    let items = listing
        .get("items")
        .and_then(Value::as_array)
        .expect("list payload has items");
    assert_eq!(items.len(), 2, "expected both facts listed: {listing}");
    assert_eq!(
        listing.get("total").and_then(Value::as_u64),
        Some(2),
        "exact total: {listing}"
    );
    let owner = items[0].get("owner").expect("listed fact has owner");
    assert_eq!(
        owner.get("type").and_then(Value::as_str),
        Some("project"),
        "owner scope: {owner}"
    );

    // Delete one real id together with a fabricated one: the real one lands in
    // `forgotten`, the fabricated one in `not_found`, and the mix must not
    // abort the call.
    let args = [
        "fact",
        "delete",
        "--workspace",
        workspace.as_str(),
        "--project",
        project.as_str(),
        first_id.as_str(),
        "fact-00000000000000000000000000000000",
    ];
    // A missing id makes the command exit non-zero (like `project document
    // import` on partial failure), but the per-id outcomes still print first.
    let output = run(&home, &args);
    assert!(
        !output.status.success(),
        "mixed-id delete must exit non-zero"
    );
    let stdout = String::from_utf8_lossy(&output.stdout).into_owned();
    let outcome: Value = serde_json::from_str(&stdout).expect("parse delete JSON");
    assert_eq!(
        outcome.get("forgotten"),
        Some(&Value::Array(vec![Value::String(first_id.clone())])),
        "forgotten ids: {outcome}"
    );
    assert_eq!(
        outcome
            .get("not_found")
            .and_then(Value::as_array)
            .map(Vec::len),
        Some(1),
        "fabricated id must be reported alongside the failure: {outcome}"
    );
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        stderr.contains("not found in the given scope"),
        "stderr names the failure: {stderr}"
    );

    // Deleting the same id again succeeds: the server's forget endpoint is
    // idempotent for ids it has seen (measured 2026-08-07, on both actor and
    // project scopes). Only an id that never existed answers NOT_FOUND, which
    // is what the fabricated id above exercised.
    let args = [
        "fact",
        "delete",
        "--workspace",
        workspace.as_str(),
        "--project",
        project.as_str(),
        first_id.as_str(),
    ];
    let stdout = assert_success(&run(&home, &args), &args);
    let outcome: Value = serde_json::from_str(&stdout).expect("parse delete JSON");
    assert_eq!(
        outcome
            .get("forgotten")
            .and_then(Value::as_array)
            .map(Vec::len),
        Some(1),
        "second delete of the same id is idempotent: {outcome}"
    );

    delete_project(&home, &workspace, &project);
    let _ = fs::remove_dir_all(&home);
}

/// A logged-in temp `$HOME` plus a scratch workspace, project, and bound
/// agent. The project and agent are removed however the test ends, including
/// a mid-test panic; the workspace is left behind (no `workspace delete`).
struct Scratch {
    home: PathBuf,
    workspace: String,
    project: String,
    agent: String,
}

impl Scratch {
    fn start() -> Self {
        let api_key = require_api_key();
        let home = temp_home();
        login_default(&home, &api_key);
        let workspace = create_workspace(&home);
        let project = create_project(&home, &workspace);

        let name = unique_name("fact-agent");
        let agent = id_of(&run_json(
            &home,
            &[
                "agent",
                "create",
                "--name",
                name.as_str(),
                "--custom-id",
                name.as_str(),
            ],
        ));
        let args = [
            "agent",
            "bind",
            agent.as_str(),
            "--workspace",
            workspace.as_str(),
        ];
        assert_success(&run(&home, &args), &args);

        Self {
            home,
            workspace,
            project,
            agent,
        }
    }

    /// Run a `fact` subcommand in the scratch workspace and parse its output.
    fn fact(&self, args: &[&str]) -> Value {
        let mut full = vec!["fact"];
        full.extend_from_slice(args);
        full.extend_from_slice(&["--workspace", self.workspace.as_str()]);
        run_json(&self.home, &full)
    }
}

impl Drop for Scratch {
    fn drop(&mut self) {
        let _ = run(
            &self.home,
            &[
                "project",
                "delete",
                "--workspace",
                &self.workspace,
                &self.project,
            ],
        );
        let _ = run(&self.home, &["agent", "delete", &self.agent]);
        let _ = fs::remove_dir_all(&self.home);
    }
}

fn run_json(home: &Path, args: &[&str]) -> Value {
    let stdout = assert_success(&run(home, args), args);
    serde_json::from_str(&stdout).unwrap_or_else(|err| panic!("{args:?} printed non-JSON: {err}"))
}

fn id_of(value: &Value) -> String {
    value
        .get("id")
        .and_then(Value::as_str)
        .unwrap_or_else(|| panic!("no id in {value}"))
        .to_string()
}

#[test]
fn fact_agent_scope_edit_trace_and_instruction() {
    let scratch = Scratch::start();
    let agent = scratch.agent.as_str();
    let project = scratch.project.as_str();

    // Agent-scoped facts: add, then read one back.
    let added = scratch.fact(&[
        "add",
        "--agent",
        agent,
        "cli-fact-live agent prefers short replies",
    ]);
    let fact_id = added["facts"][0]["id"]
        .as_str()
        .expect("added fact has an id")
        .to_string();
    let fact = scratch.fact(&["get", "--agent", agent, fact_id.as_str()]);
    assert_eq!(
        fact["fact"], "cli-fact-live agent prefers short replies",
        "{fact}"
    );

    // Edit text and metadata; metadata replaces the stored object whole.
    let updated = scratch.fact(&[
        "update",
        "--agent",
        agent,
        fact_id.as_str(),
        "--text",
        "cli-fact-live agent prefers one-line replies",
        "--metadata",
        r#"{"source":"cli-live"}"#,
    ]);
    assert_eq!(
        updated["fact"],
        "cli-fact-live agent prefers one-line replies"
    );
    assert_eq!(
        updated["metadata"],
        serde_json::json!({"source": "cli-live"})
    );
    let replaced = scratch.fact(&[
        "update",
        "--agent",
        agent,
        fact_id.as_str(),
        "--metadata",
        r#"{"other":1}"#,
    ]);
    assert_eq!(
        replaced["metadata"],
        serde_json::json!({"other": 1}),
        "metadata must be replaced, not merged: {replaced}"
    );

    // The workspace listing finds it by agent and substring, tagged as the
    // agent's.
    let listing = scratch.fact(&["list", "--agents", agent, "--query", "one-line"]);
    let items = listing["items"].as_array().expect("items");
    assert_eq!(items.len(), 1, "{listing}");
    assert_eq!(
        items[0]["owner"],
        serde_json::json!({"type": "agent", "id": agent})
    );

    // Forget it; the trace still answers, newest event first.
    let outcome = scratch.fact(&["delete", "--agent", agent, fact_id.as_str()]);
    assert_eq!(outcome["forgotten"], serde_json::json!([fact_id]));
    let trace = scratch.fact(&["trace", "--agent", agent, fact_id.as_str()]);
    let events: Vec<&str> = trace["trace"]
        .as_array()
        .expect("trace array")
        .iter()
        .filter_map(|entry| entry["event"].as_str())
        .collect();
    assert_eq!(events, ["FORGET", "UPDATE", "ADD"], "{trace}");
    assert_eq!(trace["fact"]["expired"], true, "{trace}");

    // Conflicts: an empty scope lists cleanly, and a malformed id is the
    // server's INVALID_ARGUMENT, not a NOT_FOUND.
    let conflicts = scratch.fact(&["conflict", "list", "--agent", agent, "--resolved", "false"]);
    assert!(conflicts["items"].is_array(), "{conflicts}");
    let args = [
        "fact",
        "conflict",
        "get",
        "--agent",
        agent,
        "not-a-conflict",
        "--workspace",
        scratch.workspace.as_str(),
    ];
    let output = run(&scratch.home, &args);
    assert!(
        !output.status.success(),
        "a malformed conflict id must fail"
    );
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(stderr.contains("INVALID_ARGUMENT"), "{stderr}");

    // Fact instruction round trip on the project: set, read, clear.
    let set = scratch.fact(&[
        "instruction",
        "set",
        "--project",
        project,
        "--text",
        "Records cli-fact-live deadlines.\n\n## Include\n- dates",
    ]);
    assert_eq!(
        set["fact_instruction"],
        "Records cli-fact-live deadlines.\n\n## Include\n- dates"
    );
    let got = scratch.fact(&["instruction", "get", "--project", project]);
    assert_eq!(got, set);
    let cleared = scratch.fact(&["instruction", "clear", "--project", project]);
    assert_eq!(cleared["fact_instruction"], "", "{cleared}");

    // A draft is generated but never saved.
    let draft = scratch.fact(&[
        "instruction",
        "draft",
        "--agent",
        agent,
        "--guidance",
        "Only how the assistant formats replies",
    ]);
    assert!(
        draft["fact_instruction"]
            .as_str()
            .is_some_and(|text| !text.trim().is_empty()),
        "{draft}"
    );
    let after = scratch.fact(&["instruction", "get", "--agent", agent]);
    assert_eq!(after["fact_instruction"], "", "a draft must not be saved");
}
