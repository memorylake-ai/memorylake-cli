//! Live `datasource` tests (require `MEMORYLAKE_API_KEY`).
//!
//! Read-only and error paths only: a datasource needs a working database
//! connection, which production could not provide (2026-10-09). The tests use
//! the account's default workspace rather than creating one, since workspaces
//! cannot be deleted.

use std::fs;
use std::path::Path;

use serde_json::Value;

use crate::common::{
    assert_failure, assert_success, live_base_url, login_args, require_api_key, run, temp_home,
};

pub fn login_default(home: &Path, api_key: &str) {
    let base_url = live_base_url();
    let args = login_args(api_key, "default", base_url.as_deref());
    assert_success(&run(home, &args), &args);
}

/// The account's `_sys_default_workspace`, which every account has.
///
/// The test account carries hundreds of scratch workspaces, so the default
/// one may sit several pages in.
pub fn default_workspace(home: &Path) -> String {
    let mut token: Option<String> = None;
    loop {
        let mut args = vec!["ws", "list", "--page-size", "100"];
        if let Some(token) = &token {
            args.extend(["--continuation-token", token.as_str()]);
        }
        let stdout = assert_success(&run(home, &args), &args);
        let page: Value = serde_json::from_str(&stdout).expect("ws list JSON");
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

#[test]
fn datasource_list_decodes() {
    let api_key = require_api_key();
    let home = temp_home();
    login_default(&home, &api_key);
    let workspace = default_workspace(&home);

    let args = [
        "datasource",
        "list",
        "--workspace",
        workspace.as_str(),
        "--page-size",
        "5",
    ];
    let stdout = assert_success(&run(&home, &args), &args);
    let page: Value = serde_json::from_str(&stdout).expect("list JSON");
    assert!(page["items"].is_array(), "{stdout}");
    let _ = fs::remove_dir_all(&home);
}

#[test]
fn unknown_datasources_and_connections_are_not_found() {
    let api_key = require_api_key();
    let home = temp_home();
    login_default(&home, &api_key);
    let workspace = default_workspace(&home);
    let ws = workspace.as_str();

    for args in [
        vec!["datasource", "get", "mlcli-no-such-ds", "--workspace", ws],
        vec![
            "datasource",
            "columns",
            "mlcli-no-such-ds",
            "--table",
            "t",
            "--workspace",
            ws,
        ],
        // Fails on the unknown connection, so nothing is created.
        vec![
            "datasource",
            "create",
            "--connection",
            "mlcli-no-such-connection",
            "--schema",
            "public",
            "--name",
            "mlcli-probe",
            "--workspace",
            ws,
        ],
    ] {
        let output = assert_failure(&run(&home, &args), &args);
        assert!(output.contains("NOT_FOUND"), "{args:?}: {output}");
    }
    let _ = fs::remove_dir_all(&home);
}
