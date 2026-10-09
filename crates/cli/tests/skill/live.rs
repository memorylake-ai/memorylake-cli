//! Live `skill` tests (require `MEMORYLAKE_API_KEY`).
//!
//! The lifecycle test publishes a real skill and deletes it again, including
//! when an intermediate assertion fails. It does not wait for the security
//! review: the state it lands in is reported, not asserted, because how long
//! the review takes is the server's business.

use std::fs;
use std::path::{Path, PathBuf};

use serde_json::Value;

use crate::common::{
    assert_failure, assert_success, live_base_url, login_args, require_api_key, run, temp_home,
    unique_name,
};

fn login_default(home: &Path, api_key: &str) {
    let base_url = live_base_url();
    let args = login_args(api_key, "default", base_url.as_deref());
    assert_success(&run(home, &args), &args);
}

fn parse_json(stdout: &str, what: &str) -> Value {
    serde_json::from_str(stdout).unwrap_or_else(|err| panic!("parse {what} JSON: {err}\n{stdout}"))
}

/// Deletes the skill when the test ends, including on an assertion panic.
struct SkillCleanup {
    home: PathBuf,
    id: String,
    armed: bool,
}

impl SkillCleanup {
    fn disarm(&mut self) {
        self.armed = false;
    }
}

impl Drop for SkillCleanup {
    fn drop(&mut self) {
        if self.armed {
            let _ = run(&self.home, &["skill", "delete", self.id.as_str()]);
        }
    }
}

/// CRC-32 (IEEE), as ZIP requires. Bitwise: the inputs here are tiny.
fn crc32(bytes: &[u8]) -> u32 {
    let mut crc = 0xFFFF_FFFFu32;
    for &byte in bytes {
        crc ^= u32::from(byte);
        for _ in 0..8 {
            crc = if crc & 1 == 1 {
                (crc >> 1) ^ 0xEDB8_8320
            } else {
                crc >> 1
            };
        }
    }
    !crc
}

/// A minimal ZIP archive storing `files` uncompressed.
///
/// The CLI deliberately takes a ready-made archive rather than zipping a
/// directory itself, and the test suite has no zip dependency, so the fixture
/// is built by hand: local headers, a central directory, and its end record.
fn stored_zip(files: &[(&str, &[u8])]) -> Vec<u8> {
    const DOS_DATE_1980_01_01: u16 = 0x21;
    let mut out = Vec::new();
    let mut central = Vec::new();

    for (name, data) in files {
        let offset = out.len() as u32;
        let crc = crc32(data);
        let size = data.len() as u32;
        let name_len = name.len() as u16;

        out.extend_from_slice(&0x0403_4b50u32.to_le_bytes());
        for field in [20u16, 0, 0, 0, DOS_DATE_1980_01_01] {
            out.extend_from_slice(&field.to_le_bytes());
        }
        for field in [crc, size, size] {
            out.extend_from_slice(&field.to_le_bytes());
        }
        out.extend_from_slice(&name_len.to_le_bytes());
        out.extend_from_slice(&0u16.to_le_bytes());
        out.extend_from_slice(name.as_bytes());
        out.extend_from_slice(data);

        central.extend_from_slice(&0x0201_4b50u32.to_le_bytes());
        for field in [20u16, 20, 0, 0, 0, DOS_DATE_1980_01_01] {
            central.extend_from_slice(&field.to_le_bytes());
        }
        for field in [crc, size, size] {
            central.extend_from_slice(&field.to_le_bytes());
        }
        for field in [name_len, 0, 0, 0, 0] {
            central.extend_from_slice(&field.to_le_bytes());
        }
        central.extend_from_slice(&0u32.to_le_bytes());
        central.extend_from_slice(&offset.to_le_bytes());
        central.extend_from_slice(name.as_bytes());
    }

    let central_offset = out.len() as u32;
    let entries = files.len() as u16;
    out.extend_from_slice(&central);
    out.extend_from_slice(&0x0605_4b50u32.to_le_bytes());
    for field in [0u16, 0, entries, entries] {
        out.extend_from_slice(&field.to_le_bytes());
    }
    out.extend_from_slice(&(central.len() as u32).to_le_bytes());
    out.extend_from_slice(&central_offset.to_le_bytes());
    out.extend_from_slice(&0u16.to_le_bytes());
    out
}

/// Write a skill package for `name` into a fresh scratch directory.
fn scratch_skill_package(name: &str, body: &str) -> (PathBuf, String) {
    let dir = std::env::temp_dir().join(unique_name("skill-live"));
    fs::create_dir_all(&dir).expect("create scratch dir");
    let skill_md = format!(
        "---\nname: {name}\ndescription: Replies with hello. Created by the memorylake-cli live tests.\n---\n\n{body}\n"
    );
    let path = dir.join("skill.zip");
    fs::write(&path, stored_zip(&[("SKILL.md", skill_md.as_bytes())]))
        .expect("write skill package");
    let arg = path.to_string_lossy().into_owned();
    (dir, arg)
}

#[test]
fn skill_lifecycle_create_version_update_delete() {
    let api_key = require_api_key();
    let home = temp_home();
    login_default(&home, &api_key);

    let name = unique_name("skill");
    let (dir, package) = scratch_skill_package(&name, "When asked to greet, reply with hello.");

    // Create from a local archive: upload and publish in one command.
    let args = [
        "skill",
        "create",
        "--name",
        name.as_str(),
        "--title",
        "CLI live test",
        "--description",
        "created by memorylake-cli skill live test",
        "--package",
        package.as_str(),
    ];
    let created = parse_json(&assert_success(&run(&home, &args), &args), "create");
    let id = created["id"]
        .as_str()
        .expect("create returns an id")
        .to_string();
    let mut cleanup = SkillCleanup {
        home: home.clone(),
        id: id.clone(),
        armed: true,
    };
    assert_eq!(created["name"], name.as_str());
    assert_eq!(created["latest_version"], 1);
    eprintln!(
        "skill {id} created with review state {}",
        created["latest_security_status"]
    );

    // A second skill with the same name is refused (the spec says names need
    // not be unique; production disagrees).
    let args = [
        "skill",
        "create",
        "--name",
        name.as_str(),
        "--title",
        "duplicate",
        "--package",
        package.as_str(),
    ];
    let err = assert_failure(&run(&home, &args), &args);
    assert!(err.contains("SKILL_NAME_CONFLICT"), "{err}");

    // Upload separately, then publish a version from the printed URI.
    let (dir2, package2) = scratch_skill_package(&name, "When asked to greet, reply with hi.");
    let args = ["skill", "upload", package2.as_str()];
    let uploaded = parse_json(&assert_success(&run(&home, &args), &args), "upload");
    let uri = uploaded["s3_uri"].as_str().expect("upload prints s3_uri");
    let args = [
        "skill",
        "version",
        "create",
        id.as_str(),
        "--package-uri",
        uri,
        "--changelog",
        "second",
    ];
    let version = parse_json(&assert_success(&run(&home, &args), &args), "version create");
    assert_eq!(version["version"], 2);
    assert_eq!(version["changelog"], "second");

    let args = ["skill", "version", "list", id.as_str()];
    let versions = parse_json(&assert_success(&run(&home, &args), &args), "version list");
    let numbers: Vec<u64> = versions["items"]
        .as_array()
        .expect("items")
        .iter()
        .filter_map(|item| item["version"].as_u64())
        .collect();
    assert_eq!(numbers, vec![2, 1], "newest first: {versions}");

    let args = ["skill", "version", "get", id.as_str(), "1"];
    let first = parse_json(&assert_success(&run(&home, &args), &args), "version get");
    assert_eq!(first["skill_id"], id.as_str());

    let args = [
        "skill",
        "update",
        id.as_str(),
        "--title",
        "CLI live test (renamed)",
    ];
    let updated = parse_json(&assert_success(&run(&home, &args), &args), "update");
    assert_eq!(updated["display_title"], "CLI live test (renamed)");

    let args = ["skill", "get", id.as_str()];
    let fetched = parse_json(&assert_success(&run(&home, &args), &args), "get");
    assert_eq!(fetched["latest_version"], 2);

    let args = ["skill", "delete", id.as_str()];
    assert_success(&run(&home, &args), &args);
    cleanup.disarm();

    let args = ["skill", "get", id.as_str()];
    let err = assert_failure(&run(&home, &args), &args);
    assert!(err.contains("NOT_FOUND"), "{err}");

    let _ = fs::remove_dir_all(&dir);
    let _ = fs::remove_dir_all(&dir2);
    let _ = fs::remove_dir_all(&home);
}

#[test]
fn an_archive_without_skill_md_is_refused_by_the_server() {
    let api_key = require_api_key();
    let home = temp_home();
    login_default(&home, &api_key);

    let dir = std::env::temp_dir().join(unique_name("skill-live-nomd"));
    fs::create_dir_all(&dir).expect("create scratch dir");
    let path = dir.join("skill.zip");
    fs::write(&path, stored_zip(&[("README.txt", b"no skill here")])).expect("write zip");
    let path = path.to_string_lossy().into_owned();

    let name = unique_name("skill-nomd");
    let args = [
        "skill",
        "create",
        "--name",
        name.as_str(),
        "--title",
        "no SKILL.md",
        "--package",
        path.as_str(),
    ];
    let err = assert_failure(&run(&home, &args), &args);
    assert!(err.contains("SKILL.md"), "{err}");
    assert!(err.contains("INVALID_ARGUMENT"), "{err}");

    let _ = fs::remove_dir_all(&dir);
    let _ = fs::remove_dir_all(&home);
}

#[test]
fn crc32_matches_the_reference_value() {
    // The standard check value for CRC-32/IEEE.
    assert_eq!(crc32(b"123456789"), 0xCBF4_3926);
}
