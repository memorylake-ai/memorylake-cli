//! Live `workspace` / `ws` tests (require `MEMORYLAKE_API_KEY`).

use std::fs;
use std::time::{SystemTime, UNIX_EPOCH};

use crate::common::{assert_success, live_base_url, login_args, require_api_key, run, temp_home};

fn login_default(home: &std::path::Path, api_key: &str) {
    let base_url = live_base_url();
    let args = login_args(api_key, "default", base_url.as_deref());
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

#[test]
fn get_by_id_and_custom_id() {
    let api_key = require_api_key();
    let home = temp_home();
    login_default(&home, &api_key);

    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("clock")
        .as_nanos();
    let custom_id = format!("cli-bin-get-{nanos}");
    let name = format!("CLI Bin Get {nanos}");
    let create_args = [
        "workspace",
        "create",
        "--name",
        name.as_str(),
        "--custom-id",
        custom_id.as_str(),
    ];
    let stdout = assert_success(&run(&home, &create_args), &create_args);
    // Pull the assigned id out of the JSON output.
    let created: serde_json::Value = serde_json::from_str(&stdout).expect("parse create JSON");
    let id = created
        .get("id")
        .and_then(|v| v.as_str())
        .expect("create response has id")
        .to_string();

    let by_id_args = ["ws", "get", id.as_str()];
    let stdout = assert_success(&run(&home, &by_id_args), &by_id_args);
    assert!(stdout.contains(&id), "get-by-id missing id: {stdout}");
    assert!(stdout.contains(&name), "get-by-id missing name: {stdout}");

    let by_custom_args = ["ws", "get", custom_id.as_str(), "--by-custom-id"];
    let stdout = assert_success(&run(&home, &by_custom_args), &by_custom_args);
    assert!(
        stdout.contains(&id),
        "get-by-custom-id missing id: {stdout}"
    );
    assert!(
        stdout.contains(&custom_id),
        "get-by-custom-id missing custom_id: {stdout}"
    );

    let _ = fs::remove_dir_all(&home);
}
