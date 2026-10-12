//! Offline `opendata` and `project --opendata-ids` tests.

use std::fs;

use crate::common::{assert_failure, run, temp_home};

#[test]
fn list_without_login_fails() {
    let home = temp_home();
    let args = ["opendata", "list"];
    let err = assert_failure(&run(&home, &args), &args);
    assert!(
        err.contains("not logged in") || err.contains("resolve API credentials"),
        "unexpected error output: {err}"
    );
    let _ = fs::remove_dir_all(&home);
}

#[test]
fn opendata_ids_and_clear_opendata_together_are_rejected() {
    let home = temp_home();
    let args = [
        "project",
        "update",
        "proj-1",
        "--opendata-ids",
        "clinical/trials",
        "--clear-opendata",
    ];
    let err = assert_failure(&run(&home, &args), &args);
    assert!(
        err.contains("--clear-opendata") && err.contains("--opendata-ids"),
        "the error must name both conflicting flags: {err}"
    );
    let _ = fs::remove_dir_all(&home);
}

#[test]
fn an_empty_opendata_entry_is_rejected_before_credentials() {
    let home = temp_home();
    let args = [
        "project",
        "create",
        "--name",
        "P",
        "--custom-id",
        "p-1",
        "--opendata-ids",
        "a,,b",
    ];
    let err = assert_failure(&run(&home, &args), &args);
    assert!(err.contains("empty entry"), "{err}");
    assert!(!err.contains("not logged in"), "{err}");
    let _ = fs::remove_dir_all(&home);
}
