//! Offline `skill` tests (no network; temp `$HOME` only).
//!
//! The local checks run before credentials are resolved, so a `$HOME` with no
//! login at all proves both that the check fired and that nothing reached the
//! network: a command that got further would fail with "not logged in".

use std::fs;
use std::path::PathBuf;

use crate::common::{assert_failure, assert_success, run, temp_home, unique_name};

/// A scratch directory holding `skill.zip` with `contents`.
fn scratch_package(contents: &[u8]) -> (PathBuf, PathBuf) {
    let dir = std::env::temp_dir().join(unique_name("skill-offline"));
    fs::create_dir_all(&dir).expect("create scratch dir");
    let path = dir.join("skill.zip");
    fs::write(&path, contents).expect("write scratch package");
    (dir, path)
}

fn assert_failed_locally(err: &str) {
    assert!(
        !err.contains("not logged in") && !err.contains("resolve API credentials"),
        "the local check must run before credentials are resolved: {err}"
    );
}

#[test]
fn help_lists_every_skill_subcommand() {
    let home = temp_home();
    let args = ["skill", "--help"];
    let stdout = assert_success(&run(&home, &args), &args);
    for subcommand in [
        "list", "create", "get", "update", "delete", "upload", "version",
    ] {
        assert!(
            stdout.contains(subcommand),
            "missing `{subcommand}`: {stdout}"
        );
    }

    let args = ["skill", "version", "--help"];
    let stdout = assert_success(&run(&home, &args), &args);
    for subcommand in ["create", "list", "get"] {
        assert!(
            stdout.contains(subcommand),
            "missing `{subcommand}`: {stdout}"
        );
    }
    let _ = fs::remove_dir_all(&home);
}

#[test]
fn create_help_explains_the_package_and_the_review() {
    let home = temp_home();
    let args = ["skill", "create", "--help"];
    let stdout = assert_success(&run(&home, &args), &args);
    for needle in [
        "--name",
        "--title",
        "--description",
        "--package",
        "--package-uri",
        "SKILL.md",
        "10 MiB",
        "safe",
    ] {
        assert!(stdout.contains(needle), "missing `{needle}`: {stdout}");
    }
    let _ = fs::remove_dir_all(&home);
}

#[test]
fn create_requires_exactly_one_package_source() {
    let home = temp_home();
    let args = ["skill", "create", "--name", "n", "--title", "t"];
    let err = assert_failure(&run(&home, &args), &args);
    assert!(err.contains("--package"), "{err}");

    let args = [
        "skill",
        "create",
        "--name",
        "n",
        "--title",
        "t",
        "--package",
        "a.zip",
        "--package-uri",
        "s3://b/k.zip",
    ];
    let err = assert_failure(&run(&home, &args), &args);
    assert!(err.contains("cannot be used with"), "{err}");

    let args = ["skill", "version", "create", "skill-1"];
    let err = assert_failure(&run(&home, &args), &args);
    assert!(err.contains("--package"), "{err}");
    let _ = fs::remove_dir_all(&home);
}

#[test]
fn a_file_that_is_not_a_zip_is_refused_before_login() {
    let home = temp_home();
    let (dir, path) = scratch_package(b"---\nname: x\n---\n");
    let path = path.to_string_lossy().into_owned();

    for args in [
        vec![
            "skill",
            "create",
            "--name",
            "n",
            "--title",
            "t",
            "--package",
            path.as_str(),
        ],
        vec![
            "skill",
            "version",
            "create",
            "skill-1",
            "--package",
            path.as_str(),
        ],
        vec!["skill", "upload", path.as_str()],
    ] {
        let err = assert_failure(&run(&home, &args), &args);
        assert!(err.contains("not a ZIP archive"), "{args:?}: {err}");
        assert_failed_locally(&err);
    }
    let _ = fs::remove_dir_all(&dir);
    let _ = fs::remove_dir_all(&home);
}

#[test]
fn a_directory_is_refused_with_a_hint_to_zip_it() {
    let home = temp_home();
    let (dir, _) = scratch_package(b"PK\x03\x04");
    let dir_arg = dir.to_string_lossy().into_owned();

    let args = [
        "skill",
        "create",
        "--name",
        "n",
        "--title",
        "t",
        "--package",
        dir_arg.as_str(),
    ];
    let err = assert_failure(&run(&home, &args), &args);
    assert!(err.contains("directory"), "{err}");
    assert!(err.contains("zip -r"), "{err}");
    assert_failed_locally(&err);
    let _ = fs::remove_dir_all(&dir);
    let _ = fs::remove_dir_all(&home);
}

#[test]
fn a_missing_package_is_refused_before_login() {
    let home = temp_home();
    let args = ["skill", "upload", "/definitely/not/here/skill.zip"];
    let err = assert_failure(&run(&home, &args), &args);
    assert!(err.contains("/definitely/not/here/skill.zip"), "{err}");
    assert_failed_locally(&err);
    let _ = fs::remove_dir_all(&home);
}

#[test]
fn update_without_changes_is_refused_before_login() {
    let home = temp_home();
    let args = ["skill", "update", "skill-1"];
    let err = assert_failure(&run(&home, &args), &args);
    assert!(err.contains("nothing to update"), "{err}");
    assert_failed_locally(&err);
    let _ = fs::remove_dir_all(&home);
}

#[test]
fn values_out_of_range_are_rejected_by_the_parser() {
    let home = temp_home();
    let long_name = "x".repeat(256);
    for args in [
        vec!["skill", "list", "--page-size", "0"],
        vec!["skill", "list", "--page-size", "101"],
        vec!["skill", "version", "list", "skill-1", "--page-size", "500"],
        vec![
            "skill",
            "create",
            "--name",
            "",
            "--title",
            "t",
            "--package-uri",
            "u",
        ],
        vec![
            "skill",
            "create",
            "--name",
            long_name.as_str(),
            "--title",
            "t",
            "--package-uri",
            "u",
        ],
        vec!["skill", "update", "skill-1", "--title", " "],
        vec!["skill", "version", "get", "skill-1", "latest"],
    ] {
        let err = assert_failure(&run(&home, &args), &args);
        assert!(err.contains("invalid value"), "{args:?}: {err}");
    }
    let _ = fs::remove_dir_all(&home);
}

#[test]
fn network_commands_without_login_fail() {
    let home = temp_home();
    for args in [
        vec!["skill", "list"],
        vec!["skill", "get", "skill-1"],
        vec!["skill", "delete", "skill-1"],
        vec!["skill", "version", "list", "skill-1"],
        vec!["skill", "version", "get", "skill-1", "1"],
        vec![
            "skill",
            "create",
            "--name",
            "n",
            "--title",
            "t",
            "--package-uri",
            "s3://b/k.zip",
        ],
    ] {
        let err = assert_failure(&run(&home, &args), &args);
        assert!(err.contains("not logged in"), "{args:?}: {err}");
    }
    let _ = fs::remove_dir_all(&home);
}
