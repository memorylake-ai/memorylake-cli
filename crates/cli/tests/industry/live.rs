//! Live `industry` tests (require `MEMORYLAKE_API_KEY`). Read-only.

use std::fs;

use crate::common::{assert_success, live_base_url, login_args, require_api_key, run, temp_home};

#[test]
fn list_returns_the_catalogue() {
    let api_key = require_api_key();
    let home = temp_home();
    let base_url = live_base_url();
    let login = login_args(&api_key, "default", base_url.as_deref());
    assert_success(&run(&home, &login), &login);

    let args = ["industry", "list"];
    let stdout = assert_success(&run(&home, &args), &args);
    let items: serde_json::Value = serde_json::from_str(&stdout).expect("parse list JSON");
    let items = items.as_array().expect("industry list prints an array");
    assert!(!items.is_empty(), "the catalogue should not be empty");
    assert!(
        items.iter().all(|item| item["id"].is_string()),
        "every industry needs an id: {stdout}"
    );

    let _ = fs::remove_dir_all(&home);
}
