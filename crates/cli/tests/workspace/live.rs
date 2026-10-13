//! Live `workspace` / `ws` tests (require `MEMORYLAKE_API_KEY`).

use std::fs;
use std::time::{SystemTime, UNIX_EPOCH};

use crate::common::{
    OwnedWorkspace, assert_success, live_base_url, login_args, require_api_key, run, temp_home,
};

fn login_default(home: &std::path::Path, api_key: &str) {
    let base_url = live_base_url();
    let args = login_args(api_key, "default", base_url.as_deref());
    assert_success(&run(home, &args), &args);
}

#[test]
fn list_and_create() {
    let api_key = require_api_key();
    let home = temp_home();
    login_default(&home, &api_key);

    let args = ["ws", "list", "--page-size", "1"];
    let stdout = assert_success(&run(&home, &args), &args);
    assert!(
        stdout.contains("\"items\""),
        "list JSON missing items: {stdout}"
    );

    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("clock")
        .as_nanos();
    let custom_id = format!("cli-bin-live-{nanos}");
    let name = format!("CLI Bin Live {nanos}");
    let args = [
        "workspace",
        "create",
        "--name",
        name.as_str(),
        "--custom-id",
        custom_id.as_str(),
        "--description",
        "created by memorylake-cli binary live test",
    ];
    let stdout = assert_success(&run(&home, &args), &args);
    let created: serde_json::Value = serde_json::from_str(&stdout).expect("parse create JSON");
    let _workspace = OwnedWorkspace::adopt(&home, created["id"].as_str().expect("id"), &name);
    assert!(
        stdout.contains(&custom_id),
        "create JSON missing custom_id: {stdout}"
    );
    assert!(stdout.contains(&name), "create JSON missing name: {stdout}");

    let _ = fs::remove_dir_all(&home);
}

#[test]
fn get_by_id_and_custom_id() {
    let api_key = require_api_key();
    let home = temp_home();
    login_default(&home, &api_key);

    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("clock")
        .as_nanos();
    let custom_id = format!("cli-bin-get-{nanos}");
    let name = format!("CLI Bin Get {nanos}");
    let create_args = [
        "workspace",
        "create",
        "--name",
        name.as_str(),
        "--custom-id",
        custom_id.as_str(),
    ];
    let stdout = assert_success(&run(&home, &create_args), &create_args);
    // Pull the assigned id out of the JSON output.
    let created: serde_json::Value = serde_json::from_str(&stdout).expect("parse create JSON");
    let id = created
        .get("id")
        .and_then(|v| v.as_str())
        .expect("create response has id")
        .to_string();
    let _workspace = OwnedWorkspace::adopt(&home, &id, &name);

    let by_id_args = ["ws", "get", id.as_str()];
    let stdout = assert_success(&run(&home, &by_id_args), &by_id_args);
    assert!(stdout.contains(&id), "get-by-id missing id: {stdout}");
    assert!(stdout.contains(&name), "get-by-id missing name: {stdout}");

    let by_custom_args = ["ws", "get", custom_id.as_str(), "--by-custom-id"];
    let stdout = assert_success(&run(&home, &by_custom_args), &by_custom_args);
    assert!(
        stdout.contains(&id),
        "get-by-custom-id missing id: {stdout}"
    );
    assert!(
        stdout.contains(&custom_id),
        "get-by-custom-id missing custom_id: {stdout}"
    );

    let _ = fs::remove_dir_all(&home);
}

/// `update` against the shared scratch workspace, so no workspace is created
/// just to be renamed.
#[test]
fn update_replaces_metadata_and_patches_only_what_is_given() {
    let api_key = require_api_key();
    let home = temp_home();
    login_default(&home, &api_key);
    let id = crate::common::scratch_workspace(&home);

    let stamp = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("clock")
        .as_nanos()
        .to_string();
    let description = format!("updated by the live test at {stamp}");
    let metadata = format!(r#"{{"run":"{stamp}","suite":"cli"}}"#);
    let args = [
        "ws",
        "update",
        id.as_str(),
        "--description",
        description.as_str(),
        "--metadata",
        metadata.as_str(),
    ];
    let updated: serde_json::Value =
        serde_json::from_str(&assert_success(&run(&home, &args), &args)).expect("update JSON");
    assert_eq!(updated["description"], description.as_str(), "{updated}");
    assert_eq!(
        updated["metadata"],
        serde_json::json!({"run": stamp, "suite": "cli"}),
        "{updated}"
    );

    // Metadata replaces the whole map rather than merging into it, and a
    // field left out stays as it was.
    let args = [
        "ws",
        "update",
        id.as_str(),
        "--metadata",
        r#"{"suite":"cli"}"#,
    ];
    let updated: serde_json::Value =
        serde_json::from_str(&assert_success(&run(&home, &args), &args)).expect("update JSON");
    assert_eq!(updated["metadata"], serde_json::json!({"suite": "cli"}));
    assert_eq!(updated["description"], description.as_str(), "{updated}");

    // An empty object clears it; the server then omits the key entirely,
    // which prints as `null`.
    let args = ["ws", "update", id.as_str(), "--metadata", "{}"];
    let updated: serde_json::Value =
        serde_json::from_str(&assert_success(&run(&home, &args), &args)).expect("update JSON");
    assert!(
        updated
            .get("metadata")
            .is_none_or(|m| m.is_null() || m == &serde_json::json!({})),
        "{updated}"
    );

    let args = ["ws", "update", id.as_str(), "--name", ""];
    crate::common::assert_failure(&run(&home, &args), &args);

    let _ = fs::remove_dir_all(&home);
}

#[test]
fn delete_needs_the_name_then_removes_and_forgets_the_workspace() {
    let api_key = require_api_key();
    let home = temp_home();
    login_default(&home, &api_key);

    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("clock")
        .as_nanos();
    let name = format!("CLI Bin Delete {nanos}");
    let custom_id = format!("cli-bin-delete-{nanos}");
    let args = [
        "ws",
        "create",
        "--name",
        name.as_str(),
        "--custom-id",
        custom_id.as_str(),
    ];
    let created: serde_json::Value =
        serde_json::from_str(&assert_success(&run(&home, &args), &args)).expect("create JSON");
    let id = created["id"].as_str().expect("workspace id").to_string();

    // Remember it, so the delete has something to forget.
    let args = ["ws", "use", id.as_str()];
    assert_success(&run(&home, &args), &args);

    // The wrong name is refused and the workspace survives.
    let args = ["ws", "delete", id.as_str(), "--confirm", "not its name"];
    let err = crate::common::assert_failure(&run(&home, &args), &args);
    assert!(err.contains("refusing to delete"), "{err}");
    let args = ["ws", "get", id.as_str()];
    assert_success(&run(&home, &args), &args);

    let args = ["ws", "delete", id.as_str(), "--confirm", name.as_str()];
    let stdout = assert_success(&run(&home, &args), &args);
    assert!(
        stdout.contains(&format!("Deleted workspace `{id}`")),
        "{stdout}"
    );
    assert!(stdout.contains("no longer remembers it"), "{stdout}");

    let args = ["ws", "current"];
    let stdout = assert_success(&run(&home, &args), &args);
    assert!(stdout.contains("no workspace set"), "{stdout}");

    let args = ["ws", "get", id.as_str()];
    let err = crate::common::assert_failure(&run(&home, &args), &args);
    assert!(err.contains("NOT_FOUND"), "the workspace is gone: {err}");

    // Deleting it again is the server's NOT_FOUND, not a silent success.
    let args = ["ws", "delete", id.as_str(), "--confirm", name.as_str()];
    let err = crate::common::assert_failure(&run(&home, &args), &args);
    assert!(err.contains("NOT_FOUND"), "{err}");

    let _ = fs::remove_dir_all(&home);
}

#[test]
fn the_default_workspace_is_refused() {
    let api_key = require_api_key();
    let home = temp_home();
    login_default(&home, &api_key);

    let args = ["ws", "get", "_sys_default_workspace", "--by-custom-id"];
    let default: serde_json::Value =
        serde_json::from_str(&assert_success(&run(&home, &args), &args)).expect("JSON");
    let id = default["id"].as_str().expect("id");
    let name = default["name"].as_str().expect("name");

    let args = ["ws", "delete", id, "--confirm", name];
    let err = crate::common::assert_failure(&run(&home, &args), &args);
    assert!(err.contains("default workspace"), "{err}");
    let args = ["ws", "get", id];
    assert_success(&run(&home, &args), &args);

    let _ = fs::remove_dir_all(&home);
}
