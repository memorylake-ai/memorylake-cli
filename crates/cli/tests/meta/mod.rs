//! Top-level CLI smoke tests (`version`, `--help`, …).

use std::fs;

use crate::common::{assert_success, run, temp_home};

#[test]
fn version_prints_package_version() {
    let home = temp_home();
    let args = ["version"];
    let stdout = assert_success(&run(&home, &args), &args);
    assert_eq!(stdout.trim(), env!("CARGO_PKG_VERSION"));
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
