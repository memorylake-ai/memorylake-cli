//! Shared helpers for `memorylake` binary integration tests.

use std::fs;
use std::path::{Path, PathBuf};
use std::process::{Command, Output};
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{SystemTime, UNIX_EPOCH};

pub fn bin() -> Command {
    Command::new(env!("CARGO_BIN_EXE_memorylake"))
}

pub fn temp_home() -> PathBuf {
    static NEXT: AtomicU64 = AtomicU64::new(0);
    let root = std::env::temp_dir().join(format!(
        "memorylake-cli-home-{}-{}-{}",
        std::process::id(),
        NEXT.fetch_add(1, Ordering::Relaxed),
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("clock")
            .as_nanos()
    ));
    fs::create_dir_all(&root).expect("create temp home");
    root
}

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
pub fn require_api_key() -> String {
    load_dotenv();
    std::env::var("MEMORYLAKE_API_KEY")
        .ok()
        .filter(|s| !s.is_empty())
        .expect("MEMORYLAKE_API_KEY must be set for live CLI tests")
}

pub fn run(home: &Path, args: &[&str]) -> Output {
    // Isolate CLI config (`dirs::home_dir()` → `~/.memorylake`).
    // Unix reads `HOME`; Windows uses the user profile (`USERPROFILE`), not `HOME`.
    // Clear legacy `HOMEDRIVE`/`HOMEPATH` so they cannot override the temp profile.
    bin()
        .env("HOME", home)
        .env("USERPROFILE", home)
        .env_remove("HOMEDRIVE")
        .env_remove("HOMEPATH")
        .env_remove("MEMORYLAKE_API_KEY")
        .env_remove("MEMORYLAKE_BASE_URL")
        .args(args)
        .output()
        .unwrap_or_else(|err| panic!("spawn memorylake {}: {err}", args.join(" ")))
}

pub fn assert_success(output: &Output, args: &[&str]) -> String {
    let stdout = String::from_utf8_lossy(&output.stdout).into_owned();
    let stderr = String::from_utf8_lossy(&output.stderr).into_owned();
    assert!(
        output.status.success(),
        "memorylake {} failed\nstdout:\n{stdout}\nstderr:\n{stderr}",
        args.join(" ")
    );
    stdout
}

pub fn assert_failure(output: &Output, args: &[&str]) -> String {
    let stdout = String::from_utf8_lossy(&output.stdout).into_owned();
    let stderr = String::from_utf8_lossy(&output.stderr).into_owned();
    assert!(
        !output.status.success(),
        "memorylake {} unexpectedly succeeded\nstdout:\n{stdout}\nstderr:\n{stderr}",
        args.join(" ")
    );
    format!("{stdout}{stderr}")
}
