//! Live `library` / `lib` tests (require `MEMORYLAKE_API_KEY`).
//!
//! Each test creates its own scratch folder under the workspace root and
//! deletes it at the end. Cleanup runs on the normal path only; a failing test
//! deliberately leaves its folder behind for inspection. Deletion never targets
//! anything the test did not create.

use std::fs;
use std::path::Path;

use serde_json::Value;

use crate::common::{
    assert_failure, assert_success, live_base_url, login_args, require_api_key, run, scratch_file,
    temp_home, unique_name,
};

/// Large enough to span more than one upload part (parts were 5 MiB at this
/// size when the binding was written; the assertion checks the real outcome).
const MULTIPART_BYTES: u64 = 6 * 1024 * 1024;

fn login(home: &Path, api_key: &str) {
    // `run` strips MEMORYLAKE_BASE_URL from the child environment, so a
    // non-default endpoint has to be pinned onto the temp-`$HOME` profile here
    // or every later command in the test would silently target the default.
    let base_url = live_base_url();
    let args = login_args(api_key, "default", base_url.as_deref());
    assert_success(&run(home, &args), &args);
}

fn json(stdout: &str) -> Value {
    serde_json::from_str(stdout).unwrap_or_else(|err| panic!("parse JSON output: {err}\n{stdout}"))
}

fn field<'a>(value: &'a Value, key: &str) -> &'a str {
    value
        .get(key)
        .and_then(Value::as_str)
        .unwrap_or_else(|| panic!("expected a string `{key}` in {value}"))
}

/// Create a scratch folder under the workspace root and return its item id.
fn make_scratch_folder(home: &Path, tag: &str) -> String {
    let name = unique_name(tag);
    let args = ["library", "mkdir", name.as_str(), "--on-conflict", "deny"];
    let created = json(&assert_success(&run(home, &args), &args));
    assert_eq!(field(&created, "name"), name, "deny must not rename");
    field(&created, "item_id").to_string()
}

fn delete_scratch_folder(home: &Path, item_id: &str) {
    let args = ["library", "delete", item_id];
    let stdout = assert_success(&run(home, &args), &args);
    assert!(stdout.contains("Deleted item"));
}

#[test]
fn multipart_upload_get_and_cascading_delete() {
    let api_key = require_api_key();
    let home = temp_home();
    login(&home, &api_key);
    let folder = make_scratch_folder(&home, "cli-multipart");

    let (dir, path) = scratch_file("cli-multipart", MULTIPART_BYTES);
    let source = path.to_str().expect("utf-8 scratch path");
    let args = ["lib", "upload", source, "--parent", folder.as_str()];
    let uploaded = json(&assert_success(&run(&home, &args), &args));
    let file_id = field(&uploaded, "item_id").to_string();
    assert_eq!(field(&uploaded, "name"), "payload.bin");
    let _ = fs::remove_dir_all(&dir);

    let args = ["library", "get", file_id.as_str()];
    let fetched = json(&assert_success(&run(&home, &args), &args));
    assert_eq!(
        fetched.get("size").and_then(Value::as_u64),
        Some(MULTIPART_BYTES),
        "uploaded size must match the local file exactly"
    );
    assert_eq!(fetched.get("type").and_then(Value::as_str), Some("file"));
    // A composite ETag ending in `-1` would mean the file went up as a single
    // part and the multi-part assembly path was never exercised.
    let etag = field(&fetched, "etag");
    assert!(
        etag.rsplit_once('-').is_some_and(|(_, n)| n != "1"),
        "expected a multi-part composite etag, got {etag}"
    );

    // The file must be visible in its parent listing.
    let args = ["lib", "list", folder.as_str()];
    let listing = json(&assert_success(&run(&home, &args), &args));
    let items = listing["items"].as_array().expect("items array");
    assert_eq!(items.len(), 1);
    assert_eq!(field(&items[0], "item_id"), file_id);

    delete_scratch_folder(&home, &folder);

    // Deleting the folder must take the file with it.
    let args = ["library", "get", file_id.as_str()];
    let err = assert_failure(&run(&home, &args), &args);
    assert!(
        err.contains("NOT_FOUND") || err.contains("not found"),
        "nested file should be gone after the folder delete: {err}"
    );

    let _ = fs::remove_dir_all(&home);
}

#[test]
fn single_part_upload_with_explicit_name() {
    let api_key = require_api_key();
    let home = temp_home();
    login(&home, &api_key);
    let folder = make_scratch_folder(&home, "cli-single");

    let (dir, path) = scratch_file("cli-single", 2_048);
    let source = path.to_str().expect("utf-8 scratch path");
    let args = [
        "lib",
        "upload",
        source,
        "--parent",
        folder.as_str(),
        "--name",
        "renamed.bin",
    ];
    let uploaded = json(&assert_success(&run(&home, &args), &args));
    assert_eq!(field(&uploaded, "name"), "renamed.bin");
    let _ = fs::remove_dir_all(&dir);

    let args = ["library", "get", field(&uploaded, "item_id")];
    let fetched = json(&assert_success(&run(&home, &args), &args));
    assert_eq!(fetched.get("size").and_then(Value::as_u64), Some(2_048));

    delete_scratch_folder(&home, &folder);
    let _ = fs::remove_dir_all(&home);
}

#[test]
fn list_defaults_to_the_workspace_root() {
    let api_key = require_api_key();
    let home = temp_home();
    login(&home, &api_key);

    let args = ["lib", "list", "--page-size", "1"];
    let listing = json(&assert_success(&run(&home, &args), &args));
    assert!(listing.get("items").is_some_and(Value::is_array));

    // Explicit MY_SPACE must behave identically to the default.
    let args = ["lib", "list", "MY_SPACE", "--page-size", "1"];
    let explicit = json(&assert_success(&run(&home, &args), &args));
    assert!(explicit.get("items").is_some_and(Value::is_array));

    let _ = fs::remove_dir_all(&home);
}

#[test]
fn get_accepts_the_root_alias() {
    let api_key = require_api_key();
    let home = temp_home();
    login(&home, &api_key);

    let args = ["library", "get", "MY_SPACE"];
    let root = json(&assert_success(&run(&home, &args), &args));
    assert_eq!(root.get("type").and_then(Value::as_str), Some("directory"));
    assert!(!field(&root, "item_id").is_empty());

    let _ = fs::remove_dir_all(&home);
}

#[test]
fn on_conflict_deny_surfaces_the_conflict() {
    let api_key = require_api_key();
    let home = temp_home();
    login(&home, &api_key);
    let folder = make_scratch_folder(&home, "cli-conflict");

    // Same name again under `deny`. If the strategy were sent as a query
    // parameter the server would ignore it and rename instead of failing.
    let name = {
        let args = ["library", "get", folder.as_str()];
        let item = json(&assert_success(&run(&home, &args), &args));
        field(&item, "name").to_string()
    };
    let args = ["library", "mkdir", name.as_str(), "--on-conflict", "deny"];
    let err = assert_failure(&run(&home, &args), &args);
    assert!(
        err.contains("DRIVE_ITEM_CONFLICT"),
        "expected a conflict error: {err}"
    );

    delete_scratch_folder(&home, &folder);
    let _ = fs::remove_dir_all(&home);
}

#[test]
fn upload_rejects_an_empty_file_locally() {
    let api_key = require_api_key();
    let home = temp_home();
    login(&home, &api_key);

    let (dir, path) = scratch_file("cli-empty", 0);
    let source = path.to_str().expect("utf-8 scratch path");
    let args = ["lib", "upload", source];
    let err = assert_failure(&run(&home, &args), &args);
    assert!(
        err.contains("empty file") && err.contains("at least 1 byte"),
        "unexpected error output: {err}"
    );

    let _ = fs::remove_dir_all(&dir);
    let _ = fs::remove_dir_all(&home);
}

#[test]
fn deleting_the_workspace_root_is_refused() {
    let api_key = require_api_key();
    let home = temp_home();
    login(&home, &api_key);

    let args = ["library", "delete", "MY_SPACE"];
    let err = assert_failure(&run(&home, &args), &args);
    assert!(
        err.contains("ACCESS_DENIED"),
        "the workspace root must be protected: {err}"
    );

    let _ = fs::remove_dir_all(&home);
}
