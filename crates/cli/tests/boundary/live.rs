//! Live `boundary` tests (require `MEMORYLAKE_API_KEY`).
//!
//! Boundaries are created in the account's default workspace rather than a
//! fresh one: workspaces cannot be deleted, so every scratch workspace would
//! be left behind. The boundary itself is deleted again, including when an
//! intermediate assertion fails.

use std::fs;
use std::path::{Path, PathBuf};

use serde_json::Value;

use crate::common::{
    assert_failure, assert_success, live_base_url, login_args, require_api_key, run, temp_home,
    unique_name,
};

fn login_default(home: &Path, api_key: &str) {
    let base_url = live_base_url();
    let args = login_args(api_key, "default", base_url.as_deref());
    assert_success(&run(home, &args), &args);
}

fn parse_json(stdout: &str, what: &str) -> Value {
    serde_json::from_str(stdout).unwrap_or_else(|err| panic!("parse {what} JSON: {err}\n{stdout}"))
}

/// The account's default workspace, which may sit several pages in.
fn default_workspace(home: &Path) -> String {
    let mut token: Option<String> = None;
    loop {
        let mut args = vec!["ws", "list", "--page-size", "100"];
        if let Some(token) = &token {
            args.extend(["--continuation-token", token.as_str()]);
        }
        let page = parse_json(&assert_success(&run(home, &args), &args), "ws list");
        if let Some(found) = page["items"]
            .as_array()
            .expect("items")
            .iter()
            .find(|ws| ws["custom_id"] == "_sys_default_workspace")
        {
            return found["id"].as_str().expect("workspace id").to_string();
        }
        match page["continuation_token"].as_str() {
            Some(next) if !next.is_empty() => token = Some(next.to_string()),
            _ => panic!("account has no `_sys_default_workspace` on any page"),
        }
    }
}

/// Deletes the boundary when the test ends, including on an assertion panic.
struct BoundaryCleanup {
    home: PathBuf,
    id: String,
}

impl Drop for BoundaryCleanup {
    fn drop(&mut self) {
        // Deleting an absent boundary succeeds, so this is safe to run even
        // after the test deleted it itself.
        let _ = run(&self.home, &["boundary", "delete", self.id.as_str()]);
    }
}

#[test]
fn boundary_lifecycle_create_list_get_update_delete() {
    let api_key = require_api_key();
    let home = temp_home();
    login_default(&home, &api_key);
    let workspace = default_workspace(&home);

    let name = unique_name("bnd");
    let custom_id = unique_name("bnd-cid");
    let args = [
        "boundary",
        "create",
        "--workspace",
        workspace.as_str(),
        "--name",
        name.as_str(),
        "--custom-id",
        custom_id.as_str(),
    ];
    let created = parse_json(&assert_success(&run(&home, &args), &args), "create");
    let id = created["id"]
        .as_str()
        .expect("create returns an id")
        .to_string();
    let _cleanup = BoundaryCleanup {
        home: home.clone(),
        id: id.clone(),
    };
    assert_eq!(created["workspace_id"], workspace.as_str());
    assert!(created.get("project_ids").is_none(), "{created}");

    let args = [
        "boundary",
        "list",
        "--workspace",
        workspace.as_str(),
        "--name",
        name.as_str(),
    ];
    let listed = parse_json(&assert_success(&run(&home, &args), &args), "list");
    assert!(
        listed["items"]
            .as_array()
            .expect("items")
            .iter()
            .any(|item| item["id"] == id.as_str()),
        "created boundary missing from list: {listed}"
    );

    let args = ["boundary", "get", custom_id.as_str(), "--by-custom-id"];
    let fetched = parse_json(&assert_success(&run(&home, &args), &args), "get");
    assert_eq!(fetched["id"], id.as_str());

    let renamed = format!("{name}-renamed");
    let args = [
        "boundary",
        "update",
        id.as_str(),
        "--name",
        renamed.as_str(),
        "--clear-agent",
    ];
    let updated = parse_json(&assert_success(&run(&home, &args), &args), "update");
    assert_eq!(updated["name"], renamed.as_str());
    assert!(updated.get("agent_id").is_none(), "{updated}");

    let args = ["boundary", "delete", id.as_str()];
    assert_success(&run(&home, &args), &args);

    let args = ["boundary", "get", id.as_str()];
    let err = assert_failure(&run(&home, &args), &args);
    assert!(err.contains("NOT_FOUND"), "{err}");

    let _ = fs::remove_dir_all(&home);
}

#[test]
fn a_project_outside_the_workspace_is_refused() {
    let api_key = require_api_key();
    let home = temp_home();
    login_default(&home, &api_key);
    let workspace = default_workspace(&home);

    let name = unique_name("bnd-bad");
    let args = [
        "boundary",
        "create",
        "--workspace",
        workspace.as_str(),
        "--name",
        name.as_str(),
        "--projects",
        "proj-does-not-exist",
    ];
    let err = assert_failure(&run(&home, &args), &args);
    assert!(err.contains("INVALID_ARGUMENT"), "{err}");

    let _ = fs::remove_dir_all(&home);
}
