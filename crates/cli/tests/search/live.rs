//! Live `search` tests (require `MEMORYLAKE_API_KEY`).
//!
//! The CLI cannot ingest memories, so a freshly created workspace has nothing
//! to find. These tests therefore prove that the request is accepted and the
//! response decodes into the documented shape — not that retrieval is relevant.
//! Relevance is verified manually against a workspace that already holds
//! content; see the `## Search` section of the README.

use std::fs;
use std::path::Path;

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

/// Create a workspace to search in and return its id.
///
/// The workspace is left behind: `workspace delete` is not implemented.
fn create_workspace(home: &Path) -> String {
    let name = unique_name("search-ws");
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

#[test]
fn search_succeeds_and_returns_the_documented_shape() {
    let api_key = require_api_key();
    let home = temp_home();
    login_default(&home, &api_key);
    let workspace = create_workspace(&home);

    let args = [
        "search",
        "--workspace",
        workspace.as_str(),
        "what were the quarterly revenue figures",
    ];
    let stdout = assert_success(&run(&home, &args), &args);

    let parsed: Value = serde_json::from_str(&stdout).expect("search output should be JSON");
    assert!(
        parsed.get("documents").is_some_and(Value::is_array),
        "`documents` must be an array: {stdout}"
    );
    assert!(
        parsed.get("facts").is_some_and(Value::is_array),
        "`facts` must be an array: {stdout}"
    );

    let _ = fs::remove_dir_all(&home);
}

#[test]
fn search_accepts_every_documented_filter() {
    let api_key = require_api_key();
    let home = temp_home();
    login_default(&home, &api_key);
    let workspace = create_workspace(&home);

    // The server must accept the filter names and values we send, even though
    // an empty workspace cannot match anything.
    let args = [
        "search",
        "--workspace",
        workspace.as_str(),
        "--types",
        "document,fact",
        "--top-k",
        "5",
        "revenue",
    ];
    let stdout = assert_success(&run(&home, &args), &args);
    let parsed: Value = serde_json::from_str(&stdout).expect("search output should be JSON");
    assert!(parsed.get("documents").is_some(), "{stdout}");

    let _ = fs::remove_dir_all(&home);
}

#[test]
fn search_in_an_unknown_workspace_fails() {
    let api_key = require_api_key();
    let home = temp_home();
    login_default(&home, &api_key);

    let args = [
        "search",
        "--workspace",
        "ws-does-not-exist-000000000000",
        "anything",
    ];
    let err = assert_failure(&run(&home, &args), &args);
    assert!(
        err.contains("search memories"),
        "an unknown workspace should surface the API failure: {err}"
    );

    let _ = fs::remove_dir_all(&home);
}
