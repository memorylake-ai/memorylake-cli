//! Live `project` / `proj` tests (require `MEMORYLAKE_API_KEY`).

use std::fs;
use std::path::Path;
use std::time::{SystemTime, UNIX_EPOCH};

use crate::common::{
    assert_failure, assert_success, live_base_url, login_args, require_api_key, run, temp_home,
};

fn nanos() -> u128 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("clock")
        .as_nanos()
}

/// Log in on the temp `$HOME`, pinning the endpoint when one was configured.
///
/// The URL has to be supplied here rather than through the environment: `run`
/// clears `MEMORYLAKE_BASE_URL` for the child process.
fn login_default(home: &Path, api_key: &str) {
    let base_url = live_base_url();
    let args = login_args(api_key, "default", base_url.as_deref());
    assert_success(&run(home, &args), &args);
}

/// Create a workspace to host the projects under test and return its id.
///
/// The workspace itself is left behind: `workspace delete` is not implemented.
fn create_workspace(home: &Path, label: &str) -> String {
    let name = format!("CLI Live {label} {}", nanos());
    let custom_id = format!("cli-live-{label}-{}", nanos());
    let args = [
        "workspace",
        "create",
        "--name",
        name.as_str(),
        "--custom-id",
        custom_id.as_str(),
    ];
    let stdout = assert_success(&run(home, &args), &args);
    let created: serde_json::Value =
        serde_json::from_str(&stdout).expect("parse workspace create JSON");
    created
        .get("id")
        .and_then(|v| v.as_str())
        .expect("workspace create response has id")
        .to_string()
}

#[test]
fn project_lifecycle_create_list_get_update_delete() {
    let api_key = require_api_key();
    let home = temp_home();
    login_default(&home, &api_key);
    let workspace = create_workspace(&home, "projects");

    let stamp = nanos();
    let custom_id = format!("cli-proj-{stamp}");
    let name = format!("CLI Project {stamp}");

    // Create.
    let args = [
        "project",
        "create",
        "--workspace",
        workspace.as_str(),
        "--name",
        name.as_str(),
        "--custom-id",
        custom_id.as_str(),
        "--description",
        "created by memorylake-cli project live test",
    ];
    let stdout = assert_success(&run(&home, &args), &args);
    let created: serde_json::Value = serde_json::from_str(&stdout).expect("parse create JSON");
    let id = created
        .get("id")
        .and_then(|v| v.as_str())
        .expect("create response has id")
        .to_string();
    assert_eq!(
        created.get("name").and_then(|v| v.as_str()),
        Some(name.as_str())
    );
    assert_eq!(
        created.get("custom_id").and_then(|v| v.as_str()),
        Some(custom_id.as_str())
    );

    // List, filtered by the fuzzy name.
    let args = [
        "proj",
        "list",
        "--workspace",
        workspace.as_str(),
        "--page-size",
        "50",
        "--name",
        name.as_str(),
    ];
    let stdout = assert_success(&run(&home, &args), &args);
    let listed: serde_json::Value = serde_json::from_str(&stdout).expect("parse list JSON");
    let items = listed
        .get("items")
        .and_then(|v| v.as_array())
        .expect("list response has items");
    assert!(
        items
            .iter()
            .any(|p| p.get("id").and_then(|v| v.as_str()) == Some(id.as_str())),
        "created project {id} not found in list results: {stdout}"
    );

    // Get by server-assigned id.
    let args = [
        "project",
        "get",
        "--workspace",
        workspace.as_str(),
        id.as_str(),
    ];
    let stdout = assert_success(&run(&home, &args), &args);
    assert!(stdout.contains(&id), "get-by-id missing id: {stdout}");
    assert!(stdout.contains(&name), "get-by-id missing name: {stdout}");

    // Get by caller-defined custom_id.
    let args = [
        "project",
        "get",
        "--workspace",
        workspace.as_str(),
        custom_id.as_str(),
        "--by-custom-id",
    ];
    let stdout = assert_success(&run(&home, &args), &args);
    assert!(
        stdout.contains(&id),
        "get-by-custom-id resolved to a different project: {stdout}"
    );

    // Update the name; the description must survive untouched.
    let renamed = format!("{name} Renamed");
    let args = [
        "project",
        "update",
        "--workspace",
        workspace.as_str(),
        id.as_str(),
        "--name",
        renamed.as_str(),
    ];
    let stdout = assert_success(&run(&home, &args), &args);
    let updated: serde_json::Value = serde_json::from_str(&stdout).expect("parse update JSON");
    assert_eq!(
        updated.get("name").and_then(|v| v.as_str()),
        Some(renamed.as_str()),
        "update did not apply the new name: {stdout}"
    );
    assert_eq!(
        updated.get("description").and_then(|v| v.as_str()),
        Some("created by memorylake-cli project live test"),
        "omitted --description must leave the stored description unchanged: {stdout}"
    );

    // Delete, then prove the project is really gone.
    let args = [
        "project",
        "delete",
        "--workspace",
        workspace.as_str(),
        id.as_str(),
    ];
    let stdout = assert_success(&run(&home, &args), &args);
    assert_eq!(
        stdout.trim(),
        format!("Deleted project `{id}` in workspace `{workspace}`")
    );

    let args = [
        "project",
        "get",
        "--workspace",
        workspace.as_str(),
        id.as_str(),
    ];
    let err = assert_failure(&run(&home, &args), &args);
    assert!(
        !err.is_empty(),
        "getting a deleted project must fail with a message"
    );

    let _ = fs::remove_dir_all(&home);
}

#[test]
fn get_with_unknown_project_id_fails() {
    let api_key = require_api_key();
    let home = temp_home();
    login_default(&home, &api_key);
    let workspace = create_workspace(&home, "unknown");

    let args = [
        "project",
        "get",
        "--workspace",
        workspace.as_str(),
        "proj-does-not-exist-000000000000",
    ];
    let err = assert_failure(&run(&home, &args), &args);
    assert!(
        err.contains("get project"),
        "unknown project id should surface the API failure: {err}"
    );

    let _ = fs::remove_dir_all(&home);
}
