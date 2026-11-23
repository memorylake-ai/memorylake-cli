//! What `self update` refuses before it goes to the network.

use std::fs;

use crate::common::{assert_failure, run, temp_home};

#[test]
fn a_dev_build_is_not_replaced_without_force() {
    // A release build would go on to check for updates; only the unstamped
    // build these tests run has nothing to compare against.
    if option_env!("MEMORYLAKE_RELEASE").is_some() {
        return;
    }
    let home = temp_home();
    let args = ["self", "update"];
    let output = assert_failure(&run(&home, &args), &args);
    assert!(output.contains("dev build"), "{output}");
    assert!(output.contains("--force"), "{output}");
    assert!(
        !output.contains("checking for the latest release"),
        "refused before any network call: {output}"
    );
    let _ = fs::remove_dir_all(&home);
}

#[test]
fn check_and_force_contradict_each_other() {
    let home = temp_home();
    let args = ["self", "update", "--check", "--force"];
    let output = assert_failure(&run(&home, &args), &args);
    assert!(output.contains("cannot be used with"), "{output}");
    let _ = fs::remove_dir_all(&home);
}

#[test]
fn a_version_must_look_like_a_release_tag() {
    let home = temp_home();
    for version in ["", "v1/../x", "v1?x=y"] {
        let args = ["self", "update", "--version", version];
        let output = assert_failure(&run(&home, &args), &args);
        assert!(output.contains("--version"), "{version:?}: {output}");
    }
    let _ = fs::remove_dir_all(&home);
}
