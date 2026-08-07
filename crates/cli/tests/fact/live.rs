//! Live `fact` tests (require `MEMORYLAKE_API_KEY`).
//!
//! The full lifecycle runs against a scratch project rather than an actor:
//! projects can be created and deleted per test, so nothing durable is left
//! behind, while actors are account-wide. The workspace itself is left behind
//! because `workspace delete` is not implemented.

use std::fs;
use std::path::Path;

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
