//! Shared helpers for live API integration tests.
//!
//! Include from each test binary with `mod common;`.

use std::path::PathBuf;

use memorylake_core::{Client, DEFAULT_BASE_URL, ENV_API_KEY, ENV_BASE_URL};

/// Load repo-root / crate `.env` into the process environment (best-effort).
pub fn load_dotenv() {
    let manifest_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let candidates = [
        manifest_dir.join("../../.env"),
        manifest_dir.join(".env"),
        PathBuf::from(".env"),
    ];
    for path in candidates {
        if path.is_file() {
            let _ = dotenvy::from_path(&path);
            break;
        }
    }
}

/// Require `MEMORYLAKE_API_KEY` from the environment or `.env`.
///
/// Panics when missing or empty (live tests do not skip).
pub fn require_api_key() -> String {
    load_dotenv();
    std::env::var(ENV_API_KEY)
        .ok()
        .filter(|s| !s.is_empty())
        .unwrap_or_else(|| panic!("{ENV_API_KEY} must be set for live tests"))
}

/// Optional `MEMORYLAKE_BASE_URL`, otherwise the library default.
pub fn live_base_url() -> String {
    load_dotenv();
    std::env::var(ENV_BASE_URL)
        .ok()
        .filter(|s| !s.is_empty())
        .unwrap_or_else(|| DEFAULT_BASE_URL.to_string())
}

/// Build an authenticated client for live API tests.
pub fn live_client() -> Client {
    Client::new(live_base_url(), require_api_key()).expect("build live client")
}
