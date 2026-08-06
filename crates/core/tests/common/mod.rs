//! Shared helpers for live API integration tests.
//!
//! Include from each test binary with `mod common;`.

// Each test binary compiles this whole module but uses only the helpers it
// needs, so unused-here is the normal case rather than a defect.
#![allow(dead_code)]

use std::fs::File;
use std::io::{BufWriter, Write};
use std::path::PathBuf;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{SystemTime, UNIX_EPOCH};

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

/// A name no concurrent test run will collide with.
///
/// Live tests share one real workspace, so scratch objects must be
/// distinguishable per process and per test.
pub fn unique_name(tag: &str) -> String {
    static NEXT: AtomicU64 = AtomicU64::new(0);
    format!(
        "mlcli-{tag}-{}-{}-{}",
        std::process::id(),
        NEXT.fetch_add(1, Ordering::Relaxed),
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("clock")
            .as_nanos()
    )
}

/// Create a scratch directory holding a `size`-byte file of repeating content.
///
/// Returns the directory (for the caller to remove) and the file path. Content
/// is deterministic so a mis-sliced upload shows up as a size mismatch rather
/// than as noise.
pub fn write_temp_file(tag: &str, size: u64) -> (PathBuf, PathBuf) {
    let dir = std::env::temp_dir().join(unique_name(tag));
    std::fs::create_dir_all(&dir).expect("create scratch dir");
    let path = dir.join("payload.bin");

    let file = File::create(&path).expect("create scratch file");
    let mut writer = BufWriter::new(file);
    let chunk: Vec<u8> = (0..=255u8).cycle().take(64 * 1024).collect();
    let mut remaining = size;
    while remaining > 0 {
        let take = remaining.min(chunk.len() as u64) as usize;
        writer
            .write_all(&chunk[..take])
            .expect("write scratch file");
        remaining -= take as u64;
    }
    writer.flush().expect("flush scratch file");

    (dir, path)
}
