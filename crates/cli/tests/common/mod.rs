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

/// Create a scratch directory holding a `size`-byte `payload.bin`.
///
/// Returns the directory (for the caller to remove) and the file path.
pub fn scratch_file(tag: &str, size: u64) -> (PathBuf, PathBuf) {
    use std::io::{BufWriter, Write};

    let dir = std::env::temp_dir().join(unique_name(tag));
    fs::create_dir_all(&dir).expect("create scratch dir");
    let path = dir.join("payload.bin");

    let mut writer = BufWriter::new(fs::File::create(&path).expect("create scratch file"));
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

/// Create a scratch directory holding a small plain-text `payload.txt`.
///
/// Document tests need content the server can actually ingest: the binary
/// byte cycle from [`scratch_file`] fails processing with `status: error`
/// when imported as a document, because it is not text. Returns the directory
/// (for the caller to remove) and the file path.
pub fn scratch_text_file(tag: &str) -> (PathBuf, PathBuf) {
    let dir = std::env::temp_dir().join(unique_name(tag));
    fs::create_dir_all(&dir).expect("create scratch dir");
    let path = dir.join("payload.txt");

    let line = format!("Scratch document for the `{tag}` live test.\n");
    fs::write(&path, line.repeat(64)).expect("write scratch text file");

    (dir, path)
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

/// Optional `MEMORYLAKE_BASE_URL` for live tests.
///
/// [`run`] strips the variable from the child environment so a stray value in
/// the developer's shell cannot silently retarget a test. Live suites that want
/// a non-default endpoint must therefore pass it explicitly at login, which
/// stores it on the temp-`$HOME` profile for the rest of the test.
pub fn live_base_url() -> Option<String> {
    load_dotenv();
    std::env::var("MEMORYLAKE_BASE_URL")
        .ok()
        .filter(|s| !s.trim().is_empty())
}

/// Build `auth login` arguments, pinning the endpoint when one was configured.
///
/// The CLI stores the URL on the temp-`$HOME` profile, so every later command
/// in the same test resolves to the same endpoint without repeating the flag.
pub fn login_args<'a>(
    api_key: &'a str,
    profile: &'a str,
    base_url: Option<&'a str>,
) -> Vec<&'a str> {
    let mut args = vec!["auth", "login", "--api-key", api_key, "--profile", profile];
    if let Some(url) = base_url {
        args.push("--base-url");
        args.push(url);
    }
    args
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
