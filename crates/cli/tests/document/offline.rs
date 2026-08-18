//! Offline `project document` tests (no network; temp `$HOME` only).

use std::fs;

use crate::common::stub::logged_in_home;
use crate::common::{assert_failure, assert_success, run, temp_home};

#[test]
fn help_lists_document_subcommands() {
    let home = temp_home();
    let args = ["project", "document", "--help"];
    let stdout = assert_success(&run(&home, &args), &args);
    for subcommand in ["import", "list", "get", "delete"] {
        assert!(
            stdout.contains(subcommand),
            "`project document --help` missing `{subcommand}`: {stdout}"
        );
    }
    let _ = fs::remove_dir_all(&home);
}

#[test]
fn doc_alias_resolves_to_document() {
    let home = temp_home();
    let args = ["proj", "doc", "--help"];
    let stdout = assert_success(&run(&home, &args), &args);
    assert!(
        stdout.contains("import") && stdout.contains("delete"),
        "`proj doc --help` did not resolve to the document group: {stdout}"
    );
    let _ = fs::remove_dir_all(&home);
}

#[test]
fn import_help_matches_the_documented_flags_and_defaults() {
    let home = temp_home();
    let args = ["project", "document", "import", "--help"];
    let stdout = assert_success(&run(&home, &args), &args);
    for flag in [
        "--workspace",
        "--project",
        "--recursive",
        "--max-files",
        "--wait",
        "--timeout",
    ] {
        assert!(
            stdout.contains(flag),
            "`document import` missing {flag}: {stdout}"
        );
    }
    // The two numbers the contract pins down; changing either is a visible
    // behavior change, not an implementation detail.
    assert!(
        stdout.contains("[default: 500]"),
        "--max-files must default to 500: {stdout}"
    );
    assert!(
        stdout.contains("[default: 600]"),
        "--timeout must default to 600 seconds: {stdout}"
    );
    assert!(
        stdout.contains("not cancel"),
        "import help must say a timeout leaves the import running: {stdout}"
    );
    let _ = fs::remove_dir_all(&home);
}

#[test]
fn list_help_matches_the_documented_flags() {
    let home = temp_home();
    let args = ["project", "document", "list", "--help"];
    let stdout = assert_success(&run(&home, &args), &args);
    for flag in [
        "--workspace",
        "--project",
        "--page-size",
        "--continuation-token",
        "--name",
    ] {
        assert!(
            stdout.contains(flag),
            "`document list` missing {flag}: {stdout}"
        );
    }
    let _ = fs::remove_dir_all(&home);
}

#[test]
fn delete_help_states_the_consequences_and_has_no_confirmation_flag() {
    let home = temp_home();
    let args = ["project", "document", "delete", "--help"];
    let stdout = assert_success(&run(&home, &args), &args);
    assert!(
        stdout.contains("cannot be undone"),
        "`document delete` help must state that removal is irreversible: {stdout}"
    );
    assert!(
        stdout.contains("Library"),
        "`document delete` help must say the Library files survive: {stdout}"
    );
    // Matching every other delete in this CLI: no prompt, no opt-in flag.
    for flag in ["--yes", "--force"] {
        assert!(
            !stdout.contains(flag),
            "`document delete` should not expose {flag}: {stdout}"
        );
    }
    let _ = fs::remove_dir_all(&home);
}

#[test]
fn documents_are_not_addressable_by_project_custom_id() {
    // The documents endpoints are not documented as accepting a project
    // custom_id, so the flag is deliberately absent everywhere.
    let home = temp_home();
    for verb in ["import", "list", "get", "delete"] {
        let args = ["project", "document", verb, "--help"];
        let stdout = assert_success(&run(&home, &args), &args);
        assert!(
            !stdout.contains("--by-custom-id"),
            "`document {verb}` should not expose --by-custom-id: {stdout}"
        );
    }
    let _ = fs::remove_dir_all(&home);
}

#[test]
fn every_subcommand_without_login_fails() {
    let home = temp_home();
    let invocations: [&[&str]; 4] = [
        &[
            "project",
            "document",
            "import",
            "--workspace",
            "ws-1",
            "--project",
            "proj-1",
            "sc-a:inode-b",
        ],
        &[
            "project",
            "document",
            "list",
            "--workspace",
            "ws-1",
            "--project",
            "proj-1",
        ],
        &[
            "project",
            "document",
            "get",
            "--workspace",
            "ws-1",
            "--project",
            "proj-1",
            "doc-1",
        ],
        &[
            "project",
            "document",
            "delete",
            "--workspace",
            "ws-1",
            "--project",
            "proj-1",
            "doc-1",
        ],
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
fn missing_workspace_or_project_flag_is_rejected() {
    let home = temp_home();
    for (args, missing) in [
        (
            ["project", "document", "list", "--workspace", "ws-1"].as_slice(),
            "--project",
        ),
        (
            ["project", "document", "get", "--workspace", "ws-1", "doc-1"].as_slice(),
            "--project",
        ),
    ] {
        let err = assert_failure(&run(&home, args), args);
        assert!(
            err.contains(missing),
            "missing {missing} should be reported for {args:?}: {err}"
        );
    }
    let _ = fs::remove_dir_all(&home);

    // `--workspace` is optional now that one can be remembered, so its absence
    // is a runtime error naming how to supply one, not a clap error.
    let home = logged_in_home("http://127.0.0.1:1/openapi/memorylake");
    let args = ["project", "document", "list", "--project", "proj-1"];
    let err = assert_failure(&run(&home, &args), &args);
    assert!(err.contains("no workspace given"), "{err}");
    assert!(err.contains("workspace use"), "{err}");
    let _ = fs::remove_dir_all(&home);
}

#[test]
fn import_requires_at_least_one_item_id() {
    let home = temp_home();
    let args = [
        "project",
        "document",
        "import",
        "--workspace",
        "ws-1",
        "--project",
        "proj-1",
    ];
    let err = assert_failure(&run(&home, &args), &args);
    assert!(
        err.contains("ITEM_ID"),
        "import with no ids must be rejected by argument parsing: {err}"
    );
    let _ = fs::remove_dir_all(&home);
}

#[test]
fn delete_requires_at_least_one_document_id() {
    let home = temp_home();
    let args = [
        "project",
        "document",
        "delete",
        "--workspace",
        "ws-1",
        "--project",
        "proj-1",
    ];
    let err = assert_failure(&run(&home, &args), &args);
    assert!(
        err.contains("DOCUMENT_ID"),
        "delete with no ids must be rejected by argument parsing: {err}"
    );
    let _ = fs::remove_dir_all(&home);
}
