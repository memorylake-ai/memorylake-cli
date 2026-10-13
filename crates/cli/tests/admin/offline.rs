//! Offline team-management tests (no network; temp `$HOME` only).

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
