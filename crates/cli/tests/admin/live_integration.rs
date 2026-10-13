//! Live `integration` tests (require `MEMORYLAKE_API_KEY`).
//!
//! Connecting a real bot needs real platform credentials, which a test
//! suite cannot hold. What CAN be proven live is the contract that makes the
//! command safe to try: credentials are verified (or the channel refused)
//! BEFORE anything is saved, so a bad attempt leaves nothing behind, and a
//! credential never shows up in the CLI's output.

use std::fs;

use super::live::{login_default, parse, str_field};
use crate::common::{
    assert_failure, assert_success, require_api_key, run, run_with_input, scratch_workspace,
    temp_home, unique_name,
};

#[test]
fn a_bot_with_bad_credentials_is_refused_before_anything_is_saved() {
    let api_key = require_api_key();
    let home = temp_home();
    login_default(&home, &api_key);
    let workspace = scratch_workspace(&home);

    // Real references, so the refusal is about the credentials and not about
    // a missing agent or project.
    let project_name = unique_name("im-proj");
    let args = [
        "project",
        "create",
        "--workspace",
        workspace.as_str(),
        "--name",
        project_name.as_str(),
        "--custom-id",
        project_name.as_str(),
    ];
    let project = parse(&assert_success(&run(&home, &args), &args), "project");
    let project_id = str_field(&project, "id");
    let agent_name = unique_name("im-agent");
    let args = [
        "agent",
        "create",
        "--name",
        agent_name.as_str(),
        "--custom-id",
        agent_name.as_str(),
    ];
    let agent = parse(&assert_success(&run(&home, &args), &args), "agent");
    let agent_id = str_field(&agent, "id");

    let name = unique_name("im");
    let secret = "xoxb-mlcli-live-not-a-real-token";
    for channel in ["slack", "dingtalk"] {
        let creds = match channel {
            "slack" => format!(r#"{{"botToken":"{secret}","appToken":"xapp-mlcli-live"}}"#),
            _ => format!(r#"{{"appKey":"mlcli-live","appSecret":"{secret}"}}"#),
        };
        let args = [
            "im",
            "create",
            "--channel",
            channel,
            "--workspace",
            workspace.as_str(),
            "--name",
            name.as_str(),
            "--agent",
            agent_id.as_str(),
            "--read-write-project",
            project_id.as_str(),
            "--credentials-stdin",
        ];
        let output = run_with_input(&home, &args, &creds, &[]);
        let err = assert_failure(&output, &args);
        // Refused by the platform's verification or, on a deployment where
        // the channel is off, by the channel gate — both before saving.
        assert!(
            err.contains("422") && (err.contains("rejected") || err.contains("not enabled")),
            "{channel}: {err}"
        );
        assert!(!err.contains(secret), "{channel}: the secret leaked: {err}");
    }

    let args = ["im", "list", "--workspace", workspace.as_str()];
    let page = parse(&assert_success(&run(&home, &args), &args), "im list");
    assert!(
        page["items"]
            .as_array()
            .expect("items")
            .iter()
            .all(|i| i["name"] != name.as_str()),
        "a refused create must leave nothing behind: {page}"
    );

    for args in [
        ["im", "get", "mlcli-no-such-integration"].as_slice(),
        ["im", "update", "mlcli-no-such-integration", "--disable"].as_slice(),
        ["im", "delete", "mlcli-no-such-integration"].as_slice(),
    ] {
        let err = assert_failure(&run(&home, args), args);
        assert!(err.contains("NOT_FOUND"), "{args:?}: {err}");
    }

    let args = ["agent", "delete", agent_id.as_str()];
    assert_success(&run(&home, &args), &args);
    let args = [
        "project",
        "delete",
        "--workspace",
        workspace.as_str(),
        project_id.as_str(),
    ];
    assert_success(&run(&home, &args), &args);
    let _ = fs::remove_dir_all(&home);
}
