//! Live `workspace` / `ws` tests (require `MEMORYLAKE_API_KEY`).

use std::fs;
use std::time::{SystemTime, UNIX_EPOCH};

use crate::common::{assert_success, require_api_key, run, temp_home};

fn login_default(home: &std::path::Path, api_key: &str) {
    let args = [
        "auth",
        "login",
        "api_key",
        "--api-key",
        api_key,
        "--profile",
        "default",
    ];
    assert_success(&run(home, &args), &args);
}

#[test]
fn list_and_create() {
    let api_key = require_api_key();
    let home = temp_home();
    login_default(&home, &api_key);

    let args = ["ws", "list", "--page-size", "1"];
    let stdout = assert_success(&run(&home, &args), &args);
    assert!(
        stdout.contains("\"items\""),
        "list JSON missing items: {stdout}"
    );

    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("clock")
        .as_nanos();
    let custom_id = format!("cli-bin-live-{nanos}");
    let name = format!("CLI Bin Live {nanos}");
    let args = [
        "workspace",
        "create",
        "--name",
        name.as_str(),
        "--custom-id",
        custom_id.as_str(),
        "--description",
        "created by memorylake-cli binary live test",
    ];
    let stdout = assert_success(&run(&home, &args), &args);
    assert!(
        stdout.contains(&custom_id),
        "create JSON missing custom_id: {stdout}"
    );
    assert!(stdout.contains(&name), "create JSON missing name: {stdout}");

    let _ = fs::remove_dir_all(&home);
}
