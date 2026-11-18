//! `self update` against a loopback releases server.
//!
//! Each test copies the CLI into a directory of its own and updates that copy,
//! so the binary under test is the one replaced. The "release" it installs is
//! a shell script answering `version` with the tag, which is why these run on
//! Unix only.

use std::collections::HashMap;
use std::fs;
use std::io::{Read, Write};
use std::net::TcpListener;
use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};
use std::process::{Command, Output};
use std::sync::{Arc, Mutex};

use serde_json::Value;

use crate::common::{assert_failure, assert_success, temp_home};

const TAG: &str = "v20991231";
const TARGET: &str = env!("MEMORYLAKE_TARGET");

/// The fake release: reports `$reported` for `version`, and records how it was
/// asked to refresh agent skills.
fn release_script(reported: &str) -> String {
    format!(
        "#!/bin/sh\n\
         case \"$1\" in\n\
         version) echo {reported} ;;\n\
         agent-skills) echo \"$@\" > \"$(dirname \"$0\")/skills-args\"; \
         echo '{{\"version\":\"{reported}\",\"results\":[]}}' ;;\n\
         esac\n"
    )
}

fn asset() -> String {
    format!("memorylake-{TAG}-{TARGET}.tar.gz")
}

fn tar_gz(path: &str, bytes: &[u8]) -> Vec<u8> {
    let encoder = flate2::write::GzEncoder::new(Vec::new(), flate2::Compression::fast());
    let mut builder = tar::Builder::new(encoder);
    let mut header = tar::Header::new_gnu();
    header.set_size(bytes.len() as u64);
    header.set_mode(0o755);
    header.set_cksum();
    builder
        .append_data(&mut header, path, bytes)
        .expect("append");
    builder
        .into_inner()
        .expect("finish tar")
        .finish()
        .expect("finish gzip")
}

fn sha256_hex(bytes: &[u8]) -> String {
    ring::digest::digest(&ring::digest::SHA256, bytes)
        .as_ref()
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect()
}

/// A loopback server answering GET by path, laid out like
/// `https://github.com/<repo>/releases`. Unknown paths get 404. It records
/// every path requested.
struct Releases {
    url: String,
    requested: Arc<Mutex<Vec<String>>>,
}

impl Releases {
    /// Serve `archive` as this platform's asset of [`TAG`], with `checksum`
    /// as its `.sha256`, and `/latest` redirecting to [`TAG`].
    fn serve(archive: Vec<u8>, checksum: String) -> Self {
        let listener = TcpListener::bind("127.0.0.1:0").expect("bind releases server");
        let base = format!("http://{}", listener.local_addr().expect("address"));
        let url = format!("{base}/releases");

        let mut routes: HashMap<String, (String, Vec<u8>)> = HashMap::new();
        routes.insert(
            "/releases/latest".to_string(),
            (
                format!("302 Found\r\nlocation: {url}/tag/{TAG}"),
                Vec::new(),
            ),
        );
        let download = format!("/releases/download/{TAG}/{}", asset());
        routes.insert(download.clone(), ("200 OK".to_string(), archive));
        routes.insert(
            format!("{download}.sha256"),
            ("200 OK".to_string(), checksum.into_bytes()),
        );

        let requested = Arc::new(Mutex::new(Vec::new()));
        let log = Arc::clone(&requested);
        // Never joined: it lives as long as the test process.
        std::thread::spawn(move || {
            for stream in listener.incoming() {
                let Ok(mut stream) = stream else { return };
                let mut request = Vec::new();
                let mut buf = [0u8; 1024];
                while !request.windows(4).any(|w| w == b"\r\n\r\n") {
                    match stream.read(&mut buf) {
                        Ok(0) | Err(_) => break,
                        Ok(n) => request.extend_from_slice(&buf[..n]),
                    }
                }
                let request = String::from_utf8_lossy(&request);
                let path = request.split_whitespace().nth(1).unwrap_or("").to_string();
                log.lock().expect("log").push(path.clone());
                let (status, body) = routes
                    .get(&path)
                    .cloned()
                    .unwrap_or_else(|| ("404 Not Found".to_string(), b"Not Found".to_vec()));
                let head = format!(
                    "HTTP/1.1 {status}\r\ncontent-length: {}\r\nconnection: close\r\n\r\n",
                    body.len()
                );
                let _ = stream.write_all(head.as_bytes());
                let _ = stream.write_all(&body);
            }
        });
        Self { url, requested }
    }

    /// A correct release whose binary reports `reported` for `version`.
    fn with_binary_reporting(reported: &str) -> (Self, Vec<u8>) {
        let script = release_script(reported).into_bytes();
        let archive = tar_gz(&format!("memorylake-{TAG}-{TARGET}/memorylake"), &script);
        let checksum = format!("{}  {}\n", sha256_hex(&archive), asset());
        (Self::serve(archive, checksum), script)
    }

    fn requested(&self) -> Vec<String> {
        self.requested.lock().expect("log").clone()
    }
}

/// A copy of the CLI in a directory of its own, with an isolated home.
struct Install {
    home: PathBuf,
    exe: PathBuf,
    original: Vec<u8>,
}

impl Install {
    fn new() -> Self {
        let home = temp_home();
        let dir = home.join("bin");
        fs::create_dir_all(&dir).expect("create bin dir");
        let exe = dir.join("memorylake");
        fs::copy(env!("CARGO_BIN_EXE_memorylake"), &exe).expect("copy the CLI");
        let original = fs::read(&exe).expect("read the copy");
        let exe = exe.canonicalize().expect("canonical path");
        Self {
            home,
            exe,
            original,
        }
    }

    fn run(&self, releases: &Releases, args: &[&str]) -> Output {
        Command::new(&self.exe)
            .env("MEMORYLAKE_RELEASES_URL", &releases.url)
            .env("MEMORYLAKE_CONFIG_DIR", self.home.join(".memorylake"))
            .env("MEMORYLAKE_AGENT_SKILLS_HOME", &self.home)
            .env("HOME", &self.home)
            .env_remove("HTTP_PROXY")
            .env_remove("HTTPS_PROXY")
            .env_remove("ALL_PROXY")
            .env_remove("http_proxy")
            .env_remove("https_proxy")
            .env_remove("all_proxy")
            .args(args)
            .output()
            .unwrap_or_else(|err| panic!("spawn memorylake {}: {err}", args.join(" ")))
    }

    /// Mark a skill as installed by `agent-skills install` under `.claude`.
    fn with_claude_skill(self) -> Self {
        let skill = self.home.join(".claude/skills/memorylake-cli");
        fs::create_dir_all(&skill).expect("create skill dir");
        fs::write(
            skill.join(".memorylake-skill.toml"),
            "skill = \"memorylake-cli\"\nversion = \"v1\"\ndigest = \"0\"\n",
        )
        .expect("write marker");
        self
    }

    fn assert_untouched(&self) {
        assert!(
            fs::read(&self.exe).expect("read binary") == self.original,
            "the installed binary was changed"
        );
        let leftovers: Vec<_> = fs::read_dir(self.exe.parent().expect("dir"))
            .expect("list bin dir")
            .map(|entry| entry.expect("entry").file_name())
            .filter(|name| name != "memorylake")
            .collect();
        assert!(leftovers.is_empty(), "staging left behind: {leftovers:?}");
    }
}

impl Drop for Install {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.home);
    }
}

fn json(stdout: &str) -> Value {
    serde_json::from_str(stdout).unwrap_or_else(|err| panic!("{err}: {stdout}"))
}

fn mode(path: &Path) -> u32 {
    fs::metadata(path).expect("metadata").permissions().mode() & 0o777
}

#[test]
fn update_replaces_the_binary_with_the_verified_latest_release() {
    let (releases, script) = Releases::with_binary_reporting(TAG);
    let install = Install::new().with_claude_skill();
    let mode_before = mode(&install.exe);

    let args = ["self", "update", "--force"];
    let report = json(&assert_success(&install.run(&releases, &args), &args));

    assert_eq!(report["to"], TAG);
    assert_eq!(report["updated"], true);
    assert_eq!(report["path"], install.exe.to_str().expect("utf-8 path"));
    assert_eq!(fs::read(&install.exe).expect("read binary"), script);
    assert_eq!(mode(&install.exe), mode_before, "the old mode is kept");
    assert_eq!(
        releases.requested(),
        [
            "/releases/latest".to_string(),
            format!("/releases/download/{TAG}/{}", asset()),
            format!("/releases/download/{TAG}/{}.sha256", asset()),
        ]
    );

    // Skills are refreshed by the new binary, and only where some were
    // installed before.
    let skills_args = fs::read_to_string(install.exe.with_file_name("skills-args"))
        .expect("the new binary was asked to refresh skills");
    assert_eq!(skills_args.trim(), "agent-skills install --target claude");
    assert_eq!(report["agent_skills"]["version"], TAG);
}

#[test]
fn an_explicit_version_skips_the_latest_lookup_and_skills_can_be_left_alone() {
    let (releases, script) = Releases::with_binary_reporting(TAG);
    let install = Install::new().with_claude_skill();

    // A bare date gains its `v`.
    let args = [
        "self",
        "update",
        "--force",
        "--version",
        "20991231",
        "--no-skills",
    ];
    let report = json(&assert_success(&install.run(&releases, &args), &args));

    assert_eq!(report["to"], TAG);
    assert_eq!(report["updated"], true);
    assert!(report.get("agent_skills").is_none(), "{report}");
    assert_eq!(fs::read(&install.exe).expect("read binary"), script);
    assert!(
        !releases
            .requested()
            .iter()
            .any(|path| path.ends_with("/latest")),
        "{:?}",
        releases.requested()
    );
    assert!(!install.exe.with_file_name("skills-args").exists());
}

#[test]
fn without_installed_skills_none_are_written() {
    let (releases, _) = Releases::with_binary_reporting(TAG);
    let install = Install::new();

    let args = ["self", "update", "--force"];
    let report = json(&assert_success(&install.run(&releases, &args), &args));

    assert_eq!(report["updated"], true);
    assert!(report.get("agent_skills").is_none(), "{report}");
    assert!(!install.exe.with_file_name("skills-args").exists());
}

#[test]
fn check_reports_without_downloading_or_replacing() {
    let (releases, _) = Releases::with_binary_reporting(TAG);
    let install = Install::new();

    let args = ["self", "update", "--check"];
    let report = json(&assert_success(&install.run(&releases, &args), &args));

    assert_eq!(report["to"], TAG);
    assert_eq!(report["updated"], false);
    assert_eq!(releases.requested(), ["/releases/latest"]);
    install.assert_untouched();
}

#[test]
fn a_tampered_archive_is_refused() {
    let script = release_script(TAG).into_bytes();
    let archive = tar_gz(&format!("memorylake-{TAG}-{TARGET}/memorylake"), &script);
    let checksum = format!("{}  {}\n", sha256_hex(b"something else"), asset());
    let releases = Releases::serve(archive, checksum);
    let install = Install::new();

    let args = ["self", "update", "--force"];
    let output = assert_failure(&install.run(&releases, &args), &args);

    assert!(output.contains("checksum verification failed"), "{output}");
    install.assert_untouched();
}

#[test]
fn a_binary_that_is_not_the_release_asked_for_is_refused() {
    let (releases, _) = Releases::with_binary_reporting("v20000101");
    let install = Install::new();

    let args = ["self", "update", "--force"];
    let output = assert_failure(&install.run(&releases, &args), &args);

    assert!(output.contains("reports itself as `v20000101`"), "{output}");
    install.assert_untouched();
}

#[test]
fn a_missing_release_names_the_tag_and_platform() {
    let (releases, _) = Releases::with_binary_reporting(TAG);
    let install = Install::new();

    let args = ["self", "update", "--force", "--version", "v20000101"];
    let output = assert_failure(&install.run(&releases, &args), &args);

    assert!(output.contains("HTTP 404"), "{output}");
    assert!(output.contains("v20000101"), "{output}");
    assert!(output.contains(TARGET), "{output}");
    install.assert_untouched();
}
