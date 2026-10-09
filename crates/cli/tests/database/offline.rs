//! Offline `project database` tests: help text and local checks that must
//! fail before credentials are resolved.

use std::fs;

use crate::common::{assert_failure, assert_success, run, temp_home};

#[test]
fn help_lists_every_subcommand() {
    let home = temp_home();
    let args = ["project", "database", "--help"];
    let stdout = assert_success(&run(&home, &args), &args);
    for subcommand in [
        "list",
        "create",
        "get",
        "update",
        "delete",
        "reload",
        "generate-instruction",
    ] {
        assert!(
            stdout.contains(subcommand),
            "missing `{subcommand}`: {stdout}"
        );
    }
    let _ = fs::remove_dir_all(&home);
}

#[test]
fn db_alias_resolves() {
    let home = temp_home();
    let args = ["proj", "db", "create", "--help"];
    let stdout = assert_success(&run(&home, &args), &args);
    for flag in [
        "--workspace",
        "--project",
        "--datasource",
        "--name",
        "--instruction",
        "--instruction-file",
        "--analysis-model",
    ] {
        assert!(stdout.contains(flag), "missing {flag}: {stdout}");
    }
    let _ = fs::remove_dir_all(&home);
}

#[test]
fn local_mistakes_are_reported_before_credentials() {
    let home = temp_home();
    for (args, needle) in [
        (
            vec!["project", "database", "update", "--project", "p", "db-1"],
            "nothing to update",
        ),
        (
            vec![
                "project",
                "database",
                "update",
                "--project",
                "p",
                "db-1",
                "--analysis-model",
                "am-1",
                "--clear-analysis-model",
            ],
            "cannot be used with",
        ),
        (
            vec![
                "project",
                "database",
                "create",
                "--project",
                "p",
                "--datasource",
                "ds-1",
                "--name",
                "n",
                "--instruction",
                "x",
                "--instruction-file",
                "f",
            ],
            "cannot be used with",
        ),
        (
            vec![
                "project",
                "database",
                "create",
                "--project",
                "p",
                "--datasource",
                "ds-1",
                "--name",
                "n",
                "--instruction-file",
                "/nonexistent/mlcli/instruction.md",
            ],
            "instruction file",
        ),
    ] {
        let output = assert_failure(&run(&home, &args), &args);
        assert!(output.contains(needle), "{args:?}: {output}");
        assert!(!output.contains("not logged in"), "{args:?}: {output}");
    }
    let _ = fs::remove_dir_all(&home);
}
