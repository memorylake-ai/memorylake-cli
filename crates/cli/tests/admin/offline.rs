//! Offline team-management tests (no network; temp `$HOME` only).
//!
//! Every local check must fire before credentials are resolved, so a typo
//! never costs a login prompt or a round trip.

use std::fs;

use crate::common::{assert_failure, run, temp_home};

#[test]
fn every_management_command_family_requires_login() {
    // One command per family: they all talk to the API, so none may pretend
    // to work while logged out.
    let home = temp_home();
    for args in [
        ["team", "get"].as_slice(),
        ["api-key", "list"].as_slice(),
        ["member", "list"].as_slice(),
        ["role", "list"].as_slice(),
        ["invitation", "list"].as_slice(),
        ["usage"].as_slice(),
        ["access-group", "list"].as_slice(),
        ["permission", "me"].as_slice(),
        ["access", "show", "workspace", "ws-1"].as_slice(),
        ["billing", "balance"].as_slice(),
        ["audit-log", "list"].as_slice(),
        ["integration", "list"].as_slice(),
    ] {
        let err = assert_failure(&run(&home, args), args);
        assert!(
            err.contains("not logged in") || err.contains("resolve API credentials"),
            "{args:?} should require credentials: {err}"
        );
    }
    let _ = fs::remove_dir_all(&home);
}

#[test]
fn bad_input_is_rejected_locally_before_credentials_are_needed() {
    // Not logged in: each of these must fail on its own merits, never with
    // "not logged in", which would mean the check came too late.
    let home = temp_home();
    for (args, expected) in [
        (vec!["api-key", "update", "42"], "nothing to update"),
        (
            vec!["api-key", "update", "42", "--status", "expired"],
            "invalid value",
        ),
        (
            vec!["api-key", "update", "42", "--expires-at=-1"],
            "invalid value",
        ),
        (
            vec![
                "api-key",
                "update",
                "42",
                "--allowed-ip",
                "10.0.0.1",
                "--no-ip-restriction",
            ],
            "cannot be used with",
        ),
        (
            vec!["api-key", "list", "--status", "revoked"],
            "invalid value",
        ),
        (vec!["member", "update", "prin-1"], "nothing to update"),
        (
            vec!["member", "update", "prin-1", "--max-tokens", "1374389535"],
            "invalid value",
        ),
        (vec!["member", "list", "--type", "robot"], "invalid value"),
        (
            vec!["invitation", "list", "--status", "sent"],
            "invalid value",
        ),
        (vec!["usage", "--group-by", "day"], "invalid value"),
        (vec!["usage", "records", "--kind", "chat"], "invalid value"),
        // The report's flags belong to the bare report, not to a subcommand.
        (vec!["usage", "--daily", "summary"], "cannot be used with"),
        (
            vec!["team", "create", "--name", "Lab", "--api-key-name", "k"],
            "--issue-api-key",
        ),
        (vec!["team", "create", "--name", " "], "must not be empty"),
        (vec!["team", "delete"], "--confirm"),
        (
            vec!["team", "transfer-ownership", "--to", "prin-2"],
            "--confirm",
        ),
    ] {
        let err = assert_failure(&run(&home, &args), &args);
        assert!(err.contains(expected), "{args:?}: {err}");
        assert!(!err.contains("not logged in"), "{args:?}: {err}");
    }
    let _ = fs::remove_dir_all(&home);
}

#[test]
fn bad_access_control_input_is_rejected_locally() {
    let home = temp_home();
    let long_reason = "r".repeat(256);
    let too_many: Vec<String> = (0..201).map(|i| format!("prin-{i}")).collect();
    let mut add_too_many = vec!["ag", "member", "add", "g-1"];
    add_too_many.extend(too_many.iter().map(String::as_str));
    let project_without_workspace =
        r#"[{"resource_type":"memorylake:project","mode":"instances","instances":[{"id":"p-1"}]}]"#;
    for (args, expected) in [
        // `access`: the --workspace rule and the subject.
        (
            vec!["access", "show", "workspace-actor", "act-1"],
            "needs --workspace",
        ),
        (
            vec!["access", "show", "skill", "sk-1", "--workspace", "ws-1"],
            "takes no --workspace",
        ),
        (vec!["access", "show", "folder", "f-1"], "invalid value"),
        (
            vec!["access", "grant", "workspace", "ws-1", "--reason", "r"],
            "required",
        ),
        (
            vec![
                "access",
                "grant",
                "workspace",
                "ws-1",
                "--member",
                "p",
                "--group",
                "g",
                "--reason",
                "r",
            ],
            "cannot be used with",
        ),
        (
            vec![
                "access",
                "set-level",
                "workspace",
                "ws-1",
                "--member",
                "p",
                "--level",
                "owner",
                "--reason",
                "r",
            ],
            "invalid value",
        ),
        // Reasons are audited: never blank, never over 255 bytes.
        (
            vec![
                "access",
                "grant",
                "workspace",
                "ws-1",
                "--member",
                "p",
                "--reason",
                " ",
            ],
            "must not be empty",
        ),
        (
            vec![
                "perm",
                "rule",
                "delete",
                "pr1_x",
                "--reason",
                long_reason.as_str(),
            ],
            "at most 255 bytes",
        ),
        // `access-group`.
        (vec!["ag", "update", "g-1"], "nothing to update"),
        (
            vec![
                "ag",
                "create",
                "--name",
                "Ops",
                "--base-role",
                "tenant_member",
                "--base-group",
                "g-2",
            ],
            "cannot be used with",
        ),
        (
            vec![
                "ag",
                "create",
                "--name",
                "Ops",
                "--base-role",
                "tenant_owner",
            ],
            "invalid value",
        ),
        (
            vec![
                "ag",
                "create",
                "--name",
                "Ops",
                "--permission",
                "memorylake:actor",
            ],
            "<resource_type>:<action>",
        ),
        (
            vec!["ag", "permissions", "set", "g-1", "--reason", "r"],
            "required",
        ),
        (
            vec![
                "ag",
                "permissions",
                "set",
                "g-1",
                "--none",
                "--permission",
                "memorylake:actor:actor:read",
                "--reason",
                "r",
            ],
            "cannot be used with",
        ),
        (add_too_many, "at most 200"),
        // `role`.
        (
            vec!["role", "update", "tenant_custom_1"],
            "nothing to update",
        ),
        // Scopes are parsed and checked before the request is built.
        (
            vec!["member", "scope", "set", "prin-1", "--reason", "r"],
            "required",
        ),
        (
            vec![
                "member", "scope", "set", "prin-1", "--scopes", "[]", "--reason", "r",
            ],
            "no scope entries",
        ),
        (
            vec![
                "role",
                "scope",
                "set",
                "tenant_member",
                "--scopes",
                project_without_workspace,
                "--reason",
                "r",
            ],
            "workspace_id",
        ),
        (
            vec![
                "ag",
                "scope",
                "set",
                "g-1",
                "--scopes",
                r#"[{"resource_type":"memorylake:project","mode":"instances"}]"#,
                "--reason",
                "r",
            ],
            "leave the entry out",
        ),
        (
            vec![
                "member",
                "scope",
                "set",
                "prin-1",
                "--scopes-file",
                "/nonexistent/mlcli-scopes.json",
                "--reason",
                "r",
            ],
            "read scopes file",
        ),
        // `permission rule`.
        (
            vec!["perm", "rule", "list", "--subject-id", "prin-1"],
            "required",
        ),
        (
            vec![
                "perm",
                "rule",
                "create",
                "--resource",
                "memorylake:skill",
                "--action",
                "skill:read",
                "--reason",
                "r",
            ],
            "required",
        ),
    ] {
        let err = assert_failure(&run(&home, &args), &args);
        assert!(err.contains(expected), "{args:?}: {err}");
        assert!(!err.contains("not logged in"), "{args:?}: {err}");
    }
    let _ = fs::remove_dir_all(&home);
}

#[test]
fn bad_billing_and_audit_input_is_rejected_locally() {
    let home = temp_home();
    let six_recipients = "a@x,b@x,c@x,d@x,e@x,f@x";
    let long_address = "a".repeat(501);
    for (args, expected) in [
        (vec!["billing", "alert", "set"], "required"),
        (
            vec![
                "billing",
                "alert",
                "set",
                "--percent-used",
                "50",
                "--tokens-remaining",
                "10",
            ],
            "cannot be used with",
        ),
        (
            vec!["billing", "alert", "set", "--percent-used", "100"],
            "invalid value",
        ),
        (
            vec!["billing", "alert", "set", "--tokens-remaining", "0"],
            "invalid value",
        ),
        (
            vec!["billing", "alert", "set", "--percent-used", "1,2,3,4,5,6"],
            "at most 5 alert levels",
        ),
        (
            vec![
                "billing",
                "alert",
                "set",
                "--percent-used",
                "50",
                "--recipient",
                six_recipients,
            ],
            "at most 5 recipients",
        ),
        (
            vec![
                "billing",
                "transactions",
                "--start-time",
                "200",
                "--end-time",
                "100",
            ],
            "before --end-time",
        ),
        (
            vec!["billing", "invoice", "list", "--status", "unpaid"],
            "invalid value",
        ),
        (
            vec![
                "billing",
                "invoice",
                "details",
                "set",
                "in_1",
                "--type",
                "individual",
                "--address",
                "1 Main St",
            ],
            "needs --name",
        ),
        (
            vec![
                "billing",
                "invoice",
                "details",
                "set",
                "in_1",
                "--type",
                "business",
                "--address",
                "1 Main St",
            ],
            "needs --company",
        ),
        (
            vec![
                "billing",
                "invoice",
                "details",
                "set",
                "in_1",
                "--type",
                "business",
                "--company",
                "Acme",
                "--address",
                long_address.as_str(),
            ],
            "at most 500",
        ),
        (
            vec!["billing", "order", "list", "--status", "paid"],
            "invalid value",
        ),
        (
            vec!["billing", "subscription", "preview-change", "--plan", "0"],
            "invalid value",
        ),
        (vec!["audit", "list", "--result", "ok"], "invalid value"),
        (vec!["audit", "list", "--page-size", "101"], "invalid value"),
        (
            vec!["audit", "list", "--start-time", "200", "--end-time", "100"],
            "before --end-time",
        ),
        (
            vec![
                "audit",
                "list",
                "--start-time",
                "0",
                "--end-time",
                "7948801",
            ],
            "at most 92 days",
        ),
    ] {
        let err = assert_failure(&run(&home, &args), &args);
        assert!(err.contains(expected), "{args:?}: {err}");
        assert!(!err.contains("not logged in"), "{args:?}: {err}");
    }
    let _ = fs::remove_dir_all(&home);
}

#[test]
fn bad_integration_input_is_rejected_locally() {
    let home = temp_home();
    let base = [
        "im",
        "create",
        "--channel",
        "slack",
        "--workspace",
        "ws-1",
        "--name",
        "Help",
        "--agent",
        "ag-1",
        "--read-write-project",
        "p-1",
    ];
    let with = |extra: &[&'static str]| {
        let mut args = base.to_vec();
        args.extend_from_slice(extra);
        args
    };
    let long_dataset = "d".repeat(129);
    let too_many_datasets = (0..65)
        .map(|i| format!("ds-{i}"))
        .collect::<Vec<_>>()
        .join(",");
    for (args, expected) in [
        // No way to put a credential on the command line.
        (with(&["--credentials", "{}"]), "unexpected argument"),
        (with(&[]), "credentials are needed"),
        (
            with(&["--credentials-env", "MLCLI_SURELY_UNSET_IM_CREDS"]),
            "MLCLI_SURELY_UNSET_IM_CREDS",
        ),
        (
            with(&["--credentials-file", "/nonexistent/mlcli-creds.json"]),
            "read credentials file",
        ),
        (
            with(&[
                "--credentials-env",
                "HOME",
                "--credentials-file",
                "/nonexistent/x",
            ]),
            "cannot be used with",
        ),
        (
            vec![
                "im",
                "create",
                "--channel",
                "teams",
                "--name",
                "x",
                "--agent",
                "a",
                "--read-write-project",
                "p",
            ],
            "invalid value",
        ),
        (vec!["im", "update", "im-1"], "nothing to update"),
        (
            vec!["im", "update", "im-1", "--enable", "--disable"],
            "cannot be used with",
        ),
        (
            vec!["im", "update", "im-1", "--dataset", "ds-1", "--no-datasets"],
            "cannot be used with",
        ),
        (
            vec!["im", "update", "im-1", "--dataset", long_dataset.as_str()],
            "longer than 128 bytes",
        ),
        (
            vec![
                "im",
                "update",
                "im-1",
                "--dataset",
                too_many_datasets.as_str(),
            ],
            "at most 64 datasets",
        ),
        (
            vec!["im", "update", "im-1", "--daily-message-limit=-1"],
            "invalid value",
        ),
        (vec!["im", "list", "--page-size", "0"], "invalid value"),
    ] {
        let err = assert_failure(&run(&home, &args), &args);
        assert!(err.contains(expected), "{args:?}: {err}");
        assert!(!err.contains("not logged in"), "{args:?}: {err}");
    }
    let _ = fs::remove_dir_all(&home);
}

#[test]
fn credentials_are_checked_against_the_channel_before_login() {
    // Read from stdin and refused for the wrong channel, still logged out.
    let home = temp_home();
    for (channel, creds, expected) in [
        (
            "slack",
            r#"{"appKey":"k","appSecret":"s"}"#,
            "exactly botToken and appToken",
        ),
        ("wecom", r#"{"botId":"b"}"#, "exactly botId and secret"),
        ("feishu", r#"{"appId":"a","appSecret":""}"#, "is empty"),
        ("dingtalk", "not json", "not valid JSON"),
    ] {
        let args = [
            "im",
            "create",
            "--channel",
            channel,
            "--workspace",
            "ws-1",
            "--name",
            "Help",
            "--agent",
            "ag-1",
            "--read-write-project",
            "p-1",
            "--credentials-stdin",
        ];
        let output = crate::common::run_with_input(&home, &args, creds, &[]);
        let err = assert_failure(&output, &args);
        assert!(err.contains(expected), "{channel}: {err}");
        assert!(!err.contains("not logged in"), "{channel}: {err}");
    }
    // A rotation may carry one field, but only a real one.
    let args = ["im", "update", "im-1", "--credentials-stdin"];
    let output = crate::common::run_with_input(&home, &args, r#"{"token":"x"}"#, &[]);
    let err = assert_failure(&output, &args);
    assert!(err.contains("`token` is not a credential field"), "{err}");
    let _ = fs::remove_dir_all(&home);
}
