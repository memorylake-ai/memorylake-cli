//! Live `analysis-model` tests (require `MEMORYLAKE_API_KEY`).
//!
//! Read-only and error paths only. A model has to be built on a database
//! datasource, which needs a real, reachable database the test account does
//! not have, so the create and entry success paths are covered by the wire
//! tests instead. Nothing here creates anything, so there is nothing to clean
//! up.
//!
//! `analysis-model templates` is deliberately absent: measured 2026-10-09,
//! production answers `500 INTERNAL_ERROR` for it.

use std::fs;
use std::path::Path;

use crate::common::{
    assert_failure, assert_success, live_base_url, login_args, require_api_key, run, temp_home,
};

/// A model id no account will ever hold.
const MISSING_MODEL: &str = "mlcli-no-such-analysis-model";

fn login_default(home: &Path, api_key: &str) {
    let base_url = live_base_url();
    let args = login_args(api_key, "default", base_url.as_deref());
    assert_success(&run(home, &args), &args);
}

/// The account's default workspace, which every account has.
fn default_workspace(home: &Path) -> String {
    let args = ["ws", "get", "_sys_default_workspace", "--by-custom-id"];
    let stdout = assert_success(&run(home, &args), &args);
    let workspace: serde_json::Value = serde_json::from_str(&stdout).expect("parse ws get JSON");
    workspace["id"]
        .as_str()
        .expect("default workspace has an id")
        .to_string()
}

#[test]
fn list_returns_a_page() {
    let api_key = require_api_key();
    let home = temp_home();
    login_default(&home, &api_key);
    let workspace = default_workspace(&home);

    let args = [
        "am",
        "list",
        "--workspace",
        workspace.as_str(),
        "--page-size",
        "5",
    ];
    let stdout = assert_success(&run(&home, &args), &args);
    let page: serde_json::Value = serde_json::from_str(&stdout).expect("parse list JSON");
    assert!(
        page["items"].is_array(),
        "list JSON missing items: {stdout}"
    );

    let _ = fs::remove_dir_all(&home);
}

#[test]
fn a_missing_model_is_reported_as_not_found() {
    let api_key = require_api_key();
    let home = temp_home();
    login_default(&home, &api_key);
    let workspace = default_workspace(&home);

    let get = [
        "am",
        "get",
        "--workspace",
        workspace.as_str(),
        MISSING_MODEL,
    ];
    let entries = [
        "am",
        "entry",
        "list",
        "--workspace",
        workspace.as_str(),
        "--model",
        MISSING_MODEL,
        "--entity-type",
        "few_shot",
    ];
    for args in [&get[..], &entries[..]] {
        let err = assert_failure(&run(&home, args), args);
        assert!(err.contains("NOT_FOUND"), "{args:?}: {err}");
    }

    let _ = fs::remove_dir_all(&home);
}
