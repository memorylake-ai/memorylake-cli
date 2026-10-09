//! Shared helpers for `memorylake` binary integration tests.

pub mod stub;

use std::fs;
use std::path::{Path, PathBuf};
use std::process::{Command, Output};
use std::sync::OnceLock;
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
    isolated(home, args)
        .output()
        .unwrap_or_else(|err| panic!("spawn memorylake {}: {err}", args.join(" ")))
}

/// The `memorylake` command for `args`, sandboxed to `home`.
fn isolated(home: &Path, args: &[&str]) -> Command {
    // Isolate CLI state through `MEMORYLAKE_CONFIG_DIR`, which points straight
    // at the directory holding config.toml / credentials.toml.
    //
    // Redirecting the home directory is not enough, and on Windows does not work
    // at all: `dirs::home_dir()` there calls
    // `SHGetKnownFolderPath(FOLDERID_Profile)`, which ignores both `USERPROFILE`
    // and `HOME`. Tests that relied on those variables silently read the real
    // user's config, so they could never see the credentials they had just
    // written. They are still set, so anything else resolving a home directory
    // stays inside the sandbox.
    let mut command = bin();
    command
        .env("MEMORYLAKE_CONFIG_DIR", home.join(".memorylake"))
        .env("HOME", home)
        .env("USERPROFILE", home)
        .env_remove("HOMEDRIVE")
        .env_remove("HOMEPATH")
        .env_remove("MEMORYLAKE_API_KEY")
        .env_remove("MEMORYLAKE_BASE_URL")
        .env_remove("MEMORYLAKE_WORKSPACE")
        .args(args);
    command
}

/// [`run`] with `stdin` written to the child's standard input and extra
/// environment variables set, for commands that read a document, a password,
/// or other input from `-` or the environment.
pub fn run_with_input(home: &Path, args: &[&str], stdin: &str, envs: &[(&str, &str)]) -> Output {
    use std::io::Write;
    use std::process::Stdio;

    let mut child = isolated(home, args)
        .envs(envs.iter().copied())
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .unwrap_or_else(|err| panic!("spawn memorylake {}: {err}", args.join(" ")));
    // A command that fails before reading stdin closes the pipe; that is the
    // command's outcome to report, not a test failure here.
    if let Some(mut pipe) = child.stdin.take() {
        let _ = pipe.write_all(stdin.as_bytes());
    }
    child
        .wait_with_output()
        .unwrap_or_else(|err| panic!("wait for memorylake {}: {err}", args.join(" ")))
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

/// Custom id of the one workspace that live tests needing *a* workspace share.
///
/// Fixed on purpose, with no override: tests write to this workspace (`ws
/// update` replaces its description and metadata), so it must never be
/// pointed at a workspace someone actually uses.
const SCRATCH_WORKSPACE_CUSTOM_ID: &str = "mlcli-live-scratch";

/// The shared scratch workspace's id, created the first time it is missing.
///
/// Workspaces cannot be deleted through this CLI (see issue #15), so a test
/// that creates one per run leaves the account a little more cluttered every
/// time. Tests that only need somewhere to put their own projects, folders or
/// conversations look this one up by a fixed custom id instead. `home` must
/// already be logged in.
///
/// The id is resolved once per test process: `OnceLock::get_or_init` blocks
/// concurrent callers until the first finishes, so parallel tests cannot race
/// each other into the create. Only a `NOT_FOUND` lookup leads to a create;
/// any other failure (5xx, timeout, bad key) fails the test rather than
/// guessing. A duplicate custom id is refused with `409 CUSTOM_ID_CONFLICT`
/// (measured 2026-10-09), so another process creating it first is caught by
/// looking it up again.
pub fn scratch_workspace(home: &Path) -> String {
    static ID: OnceLock<String> = OnceLock::new();
    ID.get_or_init(|| {
        let custom_id = SCRATCH_WORKSPACE_CUSTOM_ID;
        let lookup_args = ["ws", "get", custom_id, "--by-custom-id"];
        let lookup = || -> Option<String> {
            let output = run(home, &lookup_args);
            if output.status.success() {
                return Some(id_in(&output.stdout));
            }
            let err = assert_failure(&output, &lookup_args);
            assert!(
                err.contains("[NOT_FOUND]"),
                "looking up the scratch workspace failed for a reason other than its absence:\n{err}"
            );
            None
        };
        if let Some(id) = lookup() {
            return id;
        }

        let create_args = [
            "ws",
            "create",
            "--name",
            custom_id,
            "--custom-id",
            custom_id,
            "--description",
            "Shared scratch workspace for memorylake-cli live tests",
        ];
        let output = run(home, &create_args);
        if output.status.success() {
            return id_in(&output.stdout);
        }
        let err = assert_failure(&output, &create_args);
        assert!(
            err.contains("[CUSTOM_ID_CONFLICT]"),
            "creating the scratch workspace failed:\n{err}"
        );
        lookup().expect("the workspace that conflicted is now found")
    })
    .clone()
}

fn id_in(stdout: &[u8]) -> String {
    let value: serde_json::Value = serde_json::from_slice(stdout).expect("workspace JSON");
    value["id"]
        .as_str()
        .unwrap_or_else(|| panic!("workspace JSON has an id: {value}"))
        .to_string()
}
