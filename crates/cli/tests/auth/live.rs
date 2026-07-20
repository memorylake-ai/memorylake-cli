//! Live `auth` tests (require `MEMORYLAKE_API_KEY`).

use std::fs;

use crate::common::{assert_success, require_api_key, run, temp_home};

#[test]
fn refresh_validates_credentials() {
    let api_key = require_api_key();
    let home = temp_home();

    let args = [
        "auth",
        "login",
        "api_key",
        "--api-key",
        api_key.as_str(),
        "--profile",
        "default",
    ];
    assert_success(&run(&home, &args), &args);

    let args = ["auth", "refresh"];
    let stdout = assert_success(&run(&home, &args), &args);
    assert!(stdout.contains("Credentials for profile `default` are valid"));

    let _ = fs::remove_dir_all(&home);
}
