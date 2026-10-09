//! Live `db-connection` tests (require `MEMORYLAKE_API_KEY`).
//!
//! Read-only and error paths only. There is no test database to point a
//! connection at, and production rejected every create while this was written
//! (HTTP 500, 2026-10-09), so nothing here creates a connection.

use std::fs;
use std::path::Path;

use serde_json::Value;

use crate::common::{
    assert_failure, assert_success, live_base_url, login_args, require_api_key, run,
    run_with_input, temp_home,
};

fn login_default(home: &Path, api_key: &str) {
    let base_url = live_base_url();
    let args = login_args(api_key, "default", base_url.as_deref());
    assert_success(&run(home, &args), &args);
}

#[test]
fn db_connection_list_decodes() {
    let api_key = require_api_key();
    let home = temp_home();
    login_default(&home, &api_key);

    let args = ["db-connection", "list", "--page-size", "5"];
    let stdout = assert_success(&run(&home, &args), &args);
    let page: Value = serde_json::from_str(&stdout).expect("list JSON");
    assert!(page["items"].is_array(), "{stdout}");
    let _ = fs::remove_dir_all(&home);
}

#[test]
fn db_connection_get_unknown_is_not_found() {
    let api_key = require_api_key();
    let home = temp_home();
    login_default(&home, &api_key);

    for args in [
        vec!["db-connection", "get", "mlcli-no-such-connection"],
        vec!["db-connection", "schemas", "mlcli-no-such-connection"],
    ] {
        let output = assert_failure(&run(&home, &args), &args);
        assert!(output.contains("NOT_FOUND"), "{args:?}: {output}");
    }
    let _ = fs::remove_dir_all(&home);
}

#[test]
fn db_connection_test_against_a_refused_port_fails_cleanly() {
    // Nothing listens on port 1 of the server's own loopback, so this fails
    // fast — and saves nothing.
    let api_key = require_api_key();
    let home = temp_home();
    login_default(&home, &api_key);

    let args = [
        "db-connection",
        "test",
        "--host",
        "127.0.0.1",
        "--port",
        "1",
        "--username",
        "mlcli",
        "--database",
        "mlcli",
        "--password-stdin",
    ];
    let output = assert_failure(&run_with_input(&home, &args, "live-probe-pw", &[]), &args);
    assert!(output.contains("DB_CONNECTION_FAILED"), "{output}");
    assert!(!output.contains("live-probe-pw"), "{output}");
    let _ = fs::remove_dir_all(&home);
}
