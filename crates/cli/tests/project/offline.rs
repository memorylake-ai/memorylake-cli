//! Offline `project` tests (no network; temp `$HOME` only).

use std::fs;

use crate::common::stub::logged_in_home;
use crate::common::{assert_failure, assert_success, run, temp_home};

#[test]
fn help_lists_project_subcommands() {
    let home = temp_home();
    let args = ["project", "--help"];
    let stdout = assert_success(&run(&home, &args), &args);
    for subcommand in ["list", "create", "get", "update", "delete"] {
        assert!(
            stdout.contains(subcommand),
            "`project --help` missing `{subcommand}`: {stdout}"
        );
    }
    let _ = fs::remove_dir_all(&home);
}

#[test]
fn proj_alias_resolves_to_project() {
    let home = temp_home();
    let args = ["proj", "--help"];
    let stdout = assert_success(&run(&home, &args), &args);
    assert!(
        stdout.contains("list") && stdout.contains("delete"),
        "`proj --help` did not resolve to the project group: {stdout}"
    );
    let _ = fs::remove_dir_all(&home);
}

#[test]
fn list_help_matches_the_documented_flags() {
    let home = temp_home();
    let args = ["project", "list", "--help"];
    let stdout = assert_success(&run(&home, &args), &args);
    for flag in [
        "--workspace",
        "--page-size",
        "--continuation-token",
        "--name",
    ] {
        assert!(
            stdout.contains(flag),
            "`project list` missing {flag}: {stdout}"
        );
    }
    let _ = fs::remove_dir_all(&home);
}

#[test]
fn get_help_offers_by_custom_id() {
    let home = temp_home();
    let args = ["project", "get", "--help"];
    let stdout = assert_success(&run(&home, &args), &args);
    assert!(stdout.contains("--workspace"), "{stdout}");
    assert!(stdout.contains("--by-custom-id"), "{stdout}");
    let _ = fs::remove_dir_all(&home);
}

#[test]
fn create_help_exposes_no_metadata_or_industry_flags() {
    // `metadata` and `industry_ids` exist in the API but are deliberately out
    // of scope; these assertions keep them from creeping in unnoticed.
    let home = temp_home();
    let args = ["project", "create", "--help"];
    let stdout = assert_success(&run(&home, &args), &args);
    for flag in ["--workspace", "--name", "--custom-id", "--description"] {
        assert!(
            stdout.contains(flag),
            "`project create` missing {flag}: {stdout}"
        );
    }
    for flag in ["--metadata", "--industry-id", "--industry-ids"] {
        assert!(
            !stdout.contains(flag),
            "`project create` should not expose {flag}: {stdout}"
        );
    }
    let _ = fs::remove_dir_all(&home);
}

#[test]
fn update_help_exposes_only_name_and_description() {
    let home = temp_home();
    let args = ["project", "update", "--help"];
    let stdout = assert_success(&run(&home, &args), &args);
    assert!(stdout.contains("--workspace"), "{stdout}");
    assert!(stdout.contains("--name"), "{stdout}");
    assert!(stdout.contains("--description"), "{stdout}");
    for flag in ["--industry-id", "--industry-ids", "--by-custom-id"] {
        assert!(
            !stdout.contains(flag),
            "`project update` should not expose {flag}: {stdout}"
        );
    }
    let _ = fs::remove_dir_all(&home);
}

#[test]
fn delete_help_states_the_consequences_and_has_no_confirmation_flag() {
    let home = temp_home();
    let args = ["project", "delete", "--help"];
    let stdout = assert_success(&run(&home, &args), &args);
    assert!(stdout.contains("--workspace"), "{stdout}");
    assert!(
        stdout.contains("cannot be undone"),
        "`project delete` help must state that deletion is irreversible: {stdout}"
    );
    for flag in ["--yes", "--force", "--by-custom-id"] {
        assert!(
            !stdout.contains(flag),
            "`project delete` should not expose {flag}: {stdout}"
        );
    }
    let _ = fs::remove_dir_all(&home);
}

#[test]
fn every_subcommand_without_login_fails() {
    let home = temp_home();
    let invocations: [&[&str]; 5] = [
        &["project", "list", "--workspace", "ws-1"],
        &[
            "project",
            "create",
            "--workspace",
            "ws-1",
            "--name",
            "Offline",
            "--custom-id",
            "offline-1",
        ],
        &["project", "get", "--workspace", "ws-1", "proj-1"],
        &[
            "project",
            "update",
            "--workspace",
            "ws-1",
            "proj-1",
            "--name",
            "Renamed",
        ],
        &["project", "delete", "--workspace", "ws-1", "proj-1"],
    ];

    for args in invocations {
        let err = assert_failure(&run(&home, args), args);
        assert!(
            err.contains("not logged in") || err.contains("resolve API credentials"),
            "unexpected error output for {args:?}: {err}"
        );
    }

    let _ = fs::remove_dir_all(&home);
}

#[test]
fn update_without_any_field_is_not_rejected_locally() {
    // An empty update is sent to the server rather than blocked by the CLI, so
    // the only thing that can stop this invocation is the missing credential.
    let home = temp_home();
    let args = ["project", "update", "--workspace", "ws-1", "proj-1"];
    let err = assert_failure(&run(&home, &args), &args);
    assert!(
        err.contains("not logged in") || err.contains("resolve API credentials"),
        "empty update must fail on credentials, not on a local guard: {err}"
    );
    let _ = fs::remove_dir_all(&home);
}

/// A base URL nothing is listening on, so a command that reaches the network
/// fails visibly rather than silently succeeding.
const UNREACHABLE: &str = "http://127.0.0.1:1/openapi/memorylake";

#[test]
fn a_missing_workspace_is_reported_with_how_to_supply_one() {
    // `--workspace` is optional now that `workspace use` can remember one, so
    // this is a runtime check rather than a clap one. The profile here has no
    // remembered workspace, so the command must say so — and say how to fix it
    // — without contacting the API.
    let home = logged_in_home(UNREACHABLE);
    for args in [
        ["project", "list"].as_slice(),
        ["project", "get", "proj-1"].as_slice(),
        ["project", "delete", "proj-1"].as_slice(),
    ] {
        let err = assert_failure(&run(&home, args), args);
        assert!(
            err.contains("no workspace given"),
            "missing workspace should be reported for {args:?}: {err}"
        );
        assert!(
            err.contains("workspace use") && err.contains("--workspace"),
            "the error must name both ways to supply one for {args:?}: {err}"
        );
        assert!(
            !err.contains("could not connect"),
            "the check must happen before any request for {args:?}: {err}"
        );
    }
    let _ = fs::remove_dir_all(&home);
}
