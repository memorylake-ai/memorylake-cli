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
        // 0 is a valid expiry: "never". It must reach the credential check.
        ["api-key", "create", "--name", "k", "--expires-at", "0"].as_slice(),
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
        (
            vec![
                "team",
                "create",
                "--name",
                "Lab",
                "--issue-api-key",
                "--api-key-name",
                "",
            ],
            "must not be empty",
        ),
        (vec!["team", "delete"], "--confirm"),
        (
            vec!["team", "transfer-ownership", "--to", "prin-2"],
            "--confirm",
        ),
        // Pages are 1-100 on every management list.
        (vec!["api-key", "list", "--page-size", "0"], "invalid value"),
        (
            vec!["member", "list", "--page-size", "101"],
            "invalid value",
        ),
        (
            vec!["invitation", "list", "--page-size", "0"],
            "invalid value",
        ),
        (
            vec!["usage", "records", "--page-size", "101"],
            "invalid value",
        ),
        // An empty id would address the collection instead of an item.
        (vec!["api-key", "revoke", ""], "must not be empty"),
        (vec!["member", "remove", " "], "must not be empty"),
        (vec!["invitation", "get", ""], "must not be empty"),
        (
            vec!["member", "set-role", "prin-1", "--role", ""],
            "must not be empty",
        ),
        (
            vec!["api-key", "rotate", "42", "--idempotency-key", ""],
            "must not be empty",
        ),
        (vec!["team", "rename", "--name", ""], "must not be empty"),
        // An expiry in the past would mint a key that is already expired.
        (
            vec!["api-key", "create", "--name", "k", "--expires-at", "1"],
            "not in the future",
        ),
        // "" would read as "no member" and issue the key for the caller.
        (
            vec!["api-key", "create", "--name", "k", "--member", ""],
            "must not be empty",
        ),
        (
            vec![
                "usage",
                "--start-date",
                "2026-01-01",
                "--end-date",
                "2026-08-27",
            ],
            "more than 92 days",
        ),
        (
            vec![
                "usage",
                "--start-date",
                "2026-05-03",
                "--end-date",
                "2026-05-02",
            ],
            "is after",
        ),
        (vec!["usage", "--end-date", "2026-02-30"], "YYYY-MM-DD"),
        (vec!["usage", "--start-date", "2026-13-01"], "YYYY-MM-DD"),
        (
            vec![
                "usage",
                "records",
                "--start-time",
                "1000",
                "--end-time",
                "1000",
            ],
            "must be before",
        ),
        (
            vec![
                "usage",
                "records",
                "--start-time",
                "1",
                "--end-time",
                "8000000",
            ],
            "92 days",
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
    let only_shadowed = r#"[{"resource_type":"memorylake:project","mode":"instances","instances":[{"id":"p-1","actions":["project:write"],"shadowed_actions":["project:write"]}]}]"#;
    let all_projects = r#"[{"resource_type":"memorylake:project","mode":"all"}]"#;
    for (args, expected) in [
        // `access`: the --workspace rule and the subject.
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
                only_shadowed,
                "--reason",
                "r",
            ],
            "no longer grants",
        ),
        (
            vec![
                "role",
                "scope",
                "set",
                "tenant_member",
                "--scopes",
                all_projects,
                "--reason",
                "r",
                "--if-match",
                r#"W/"v1""#,
            ],
            "weak ETag",
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
            "Leave the entry out",
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
        // Empty ids would address the collection; pages are 1-100.
        (vec!["ag", "delete", ""], "must not be empty"),
        (vec!["role", "get", " "], "must not be empty"),
        (vec!["member", "scope", "get", ""], "must not be empty"),
        (vec!["perm", "rule", "delete", ""], "must not be empty"),
        (
            vec!["ag", "member", "remove", "g-1", ""],
            "must not be empty",
        ),
        (vec!["ag", "list", "--page-size", "0"], "invalid value"),
        (
            vec!["ag", "member", "list", "g-1", "--page-size", "101"],
            "invalid value",
        ),
        (
            vec!["perm", "rule", "list", "--page-size", "0"],
            "invalid value",
        ),
        (
            vec![
                "ag",
                "permissions",
                "set",
                "g-1",
                "--none",
                "--reason",
                "r",
                "--if-match",
                " ",
            ],
            "must not be empty",
        ),
        (
            vec![
                "ag",
                "create",
                "--name",
                "Ops",
                "--permission",
                "memorylake:actor::create",
            ],
            "is not <resource_type>:<action>",
        ),
    ] {
        let err = assert_failure(&run(&home, &args), &args);
        assert!(err.contains(expected), "{args:?}: {err}");
        assert!(!err.contains("not logged in"), "{args:?}: {err}");
    }
    let _ = fs::remove_dir_all(&home);
}
