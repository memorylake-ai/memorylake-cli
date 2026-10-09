//! Live `fact` tests (require `MEMORYLAKE_API_KEY`).
//!
//! Both tests run in one scratch workspace, created on first use and shared
//! through [`shared_workspace`]: a workspace cannot be deleted (there is no
//! `workspace delete`), so every extra one is left behind for good. Each test
//! then creates its own project — and, for the agent scope, its own agent —
//! under a [`Scratch`] guard that removes them however the test ends.
//! Actors are not used: they are account-wide.
//!
//! Conflict *resolution* is not exercised live: detection is asynchronous and
//! its outcome is a model's judgment, so a test waiting on it would be slow
//! and flaky. Listing and the error path are covered instead.

use std::fs;
use std::path::{Path, PathBuf};
use std::sync::OnceLock;

use serde_json::Value;

use crate::common::{
    assert_success, live_base_url, login_args, require_api_key, run, temp_home, unique_name,
};

fn login_default(home: &Path, api_key: &str) {
    let base_url = live_base_url();
    let args = login_args(api_key, "default", base_url.as_deref());
    assert_success(&run(home, &args), &args);
}

/// The scratch workspace every test in this module shares, created by the
/// first caller. Left behind — there is no delete.
fn shared_workspace(home: &Path) -> String {
    static WORKSPACE: OnceLock<String> = OnceLock::new();
    WORKSPACE
        .get_or_init(|| {
            let name = unique_name("fact-ws");
            id_of(&run_json(
                home,
                &[
                    "workspace",
                    "create",
                    "--name",
                    name.as_str(),
                    "--custom-id",
                    name.as_str(),
                ],
            ))
        })
        .clone()
}

/// A logged-in temp `$HOME` in the shared workspace, plus the scratch objects
/// a test creates there.
///
/// The guard exists before anything is created and records each object as
/// soon as it exists, so a failure at any step — including a later creation —
/// still removes everything created so far.
struct Scratch {
    home: PathBuf,
    workspace: String,
    project: Option<String>,
    agent: Option<String>,
}

impl Scratch {
    /// Log in and join the shared workspace.
    fn start() -> Self {
        let api_key = require_api_key();
        let mut scratch = Self {
            home: temp_home(),
            workspace: String::new(),
            project: None,
            agent: None,
        };
        login_default(&scratch.home, &api_key);
        scratch.workspace = shared_workspace(&scratch.home);
        scratch
    }

    /// Create a scratch project in the workspace and return its id.
    fn create_project(&mut self) -> String {
        let name = unique_name("fact-proj");
        let project = id_of(&run_json(
            &self.home,
            &[
                "project",
                "create",
                "--workspace",
                self.workspace.as_str(),
                "--name",
                name.as_str(),
                "--custom-id",
                name.as_str(),
            ],
        ));
        self.project = Some(project.clone());
        project
    }

    /// Create a scratch agent, bind it to the workspace, and return its id.
    fn create_agent(&mut self) -> String {
        let name = unique_name("fact-agent");
        let agent = id_of(&run_json(
            &self.home,
            &[
                "agent",
                "create",
                "--name",
                name.as_str(),
                "--custom-id",
                name.as_str(),
            ],
        ));
        self.agent = Some(agent.clone());
        let args = [
            "agent",
            "bind",
            agent.as_str(),
            "--workspace",
            self.workspace.as_str(),
        ];
        assert_success(&run(&self.home, &args), &args);
        agent
    }

    /// Run a `fact` subcommand in the workspace and parse its output.
    fn fact(&self, args: &[&str]) -> Value {
        let mut full = vec!["fact"];
        full.extend_from_slice(args);
        full.extend_from_slice(&["--workspace", self.workspace.as_str()]);
        run_json(&self.home, &full)
    }

    /// Run a `fact` subcommand in the workspace without asserting success.
    fn fact_output(&self, args: &[&str]) -> std::process::Output {
        let mut full = vec!["fact"];
        full.extend_from_slice(args);
        full.extend_from_slice(&["--workspace", self.workspace.as_str()]);
        run(&self.home, &full)
    }

    /// Run a cleanup command, reporting (not raising) a failure: panicking
    /// during an unwind would abort and bury the assertion that failed.
    fn clean_up(&self, args: &[&str]) {
        let output = run(&self.home, args);
        if !output.status.success() {
            eprintln!(
                "cleanup: `memorylake {}` failed:\n{}",
                args.join(" "),
                String::from_utf8_lossy(&output.stderr)
            );
        }
    }
}

impl Drop for Scratch {
    fn drop(&mut self) {
        if let Some(project) = &self.project {
            self.clean_up(&["project", "delete", "--workspace", &self.workspace, project]);
        }
        if let Some(agent) = &self.agent {
            self.clean_up(&["agent", "delete", agent]);
        }
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
fn fact_lifecycle_add_list_delete() {
    let mut scratch = Scratch::start();
    let project = scratch.create_project();
    let project = project.as_str();

    // Add two facts in one call; each must come back with an id.
    let created = scratch.fact(&[
        "add",
        "--project",
        project,
        "cli-fact-live test fact one",
        "cli-fact-live test fact two",
    ]);
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
    let listing = scratch.fact(&["list", "--projects", project]);
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
    // A missing id makes the command exit non-zero (like `project document
    // import` on partial failure), but the per-id outcomes still print first.
    let output = scratch.fact_output(&[
        "delete",
        "--project",
        project,
        first_id.as_str(),
        "fact-00000000000000000000000000000000",
    ]);
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
    let outcome = scratch.fact(&["delete", "--project", project, first_id.as_str()]);
    assert_eq!(
        outcome
            .get("forgotten")
            .and_then(Value::as_array)
            .map(Vec::len),
        Some(1),
        "second delete of the same id is idempotent: {outcome}"
    );
}

#[test]
fn fact_agent_scope_edit_trace_and_instruction() {
    let mut scratch = Scratch::start();
    let project = scratch.create_project();
    let agent = scratch.create_agent();
    let (project, agent) = (project.as_str(), agent.as_str());

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

    // Forget it; the trace still answers, newest event first. Only the
    // newest event's position is pinned: the server may record more history
    // than this test made.
    let outcome = scratch.fact(&["delete", "--agent", agent, fact_id.as_str()]);
    assert_eq!(outcome["forgotten"], serde_json::json!([fact_id]));
    let trace = scratch.fact(&["trace", "--agent", agent, fact_id.as_str()]);
    let events: Vec<&str> = trace["trace"]
        .as_array()
        .expect("trace array")
        .iter()
        .filter_map(|entry| entry["event"].as_str())
        .collect();
    assert_eq!(events.first(), Some(&"FORGET"), "{trace}");
    assert!(events.contains(&"UPDATE"), "{trace}");
    assert!(events.contains(&"ADD"), "{trace}");
    assert_eq!(trace["fact"]["expired"], true, "{trace}");

    // Conflicts: an empty scope lists cleanly, and a malformed id fails.
    let conflicts = scratch.fact(&["conflict", "list", "--agent", agent, "--resolved", "false"]);
    assert!(conflicts["items"].is_array(), "{conflicts}");
    let output = scratch.fact_output(&["conflict", "get", "--agent", agent, "not-a-conflict"]);
    assert!(
        !output.status.success(),
        "a malformed conflict id must fail"
    );

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
