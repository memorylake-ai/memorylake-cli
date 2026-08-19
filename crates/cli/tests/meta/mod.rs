//! Top-level CLI smoke tests (`version`, `--help`, …).

use std::fs;

use crate::common::{assert_success, run, temp_home};

#[test]
fn version_identifies_the_build() {
    // A released binary reports its tag; anything else says it is a dev build.
    // The crate version alone cannot answer "which build is this?" — it has
    // stayed at 0.1.0 across every release, so someone who just upgraded could
    // not tell whether it worked.
    let home = temp_home();
    let args = ["version"];
    let stdout = assert_success(&run(&home, &args), &args);
    let reported = stdout.trim();
    assert!(!reported.is_empty(), "version must report something");

    match option_env!("MEMORYLAKE_RELEASE") {
        Some(release) => assert_eq!(reported, release, "a stamped build reports its release tag"),
        None => {
            assert!(
                reported.contains(env!("CARGO_PKG_VERSION")),
                "an unstamped build still names the crate version: {reported}"
            );
            assert!(
                reported.contains("dev"),
                "an unstamped build must not pass for a release: {reported}"
            );
        }
    }
    let _ = fs::remove_dir_all(&home);
}

#[test]
fn the_version_flag_and_subcommand_agree() {
    // Two code paths report the version; a user checking one and reporting the
    // other would be describing a different build.
    let home = temp_home();
    let subcommand = assert_success(&run(&home, &["version"]), &["version"]);
    let flag = assert_success(&run(&home, &["--version"]), &["--version"]);
    assert!(
        flag.trim().ends_with(subcommand.trim()),
        "`--version` ({}) and `version` ({}) disagree",
        flag.trim(),
        subcommand.trim()
    );
    let _ = fs::remove_dir_all(&home);
}

#[test]
fn help_lists_top_level_commands() {
    let home = temp_home();
    let args = ["--help"];
    let stdout = assert_success(&run(&home, &args), &args);
    assert!(stdout.contains("auth"));
    assert!(stdout.contains("workspace"));
    assert!(stdout.contains("project"));
    assert!(stdout.contains("agent"));
    assert!(stdout.contains("search"));
    assert!(stdout.contains("version"));
    let _ = fs::remove_dir_all(&home);
}
