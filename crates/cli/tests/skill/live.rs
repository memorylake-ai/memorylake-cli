//! Live `skill` tests (require `MEMORYLAKE_API_KEY`).
//!
//! The lifecycle test publishes a real skill and deletes it again, including
//! when an intermediate assertion fails. It does not wait for the security
//! review: the state it lands in is reported, not asserted, because how long
//! the review takes is the server's business.
//!
//! Negative cases expect the server to refuse a create. If it unexpectedly
//! accepts one, the skill it made is deleted before the test fails, so a
//! behaviour change on the server does not leave skills behind.

use std::fs;
use std::path::{Path, PathBuf};

use serde_json::Value;

use super::zip::stored_zip;
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

/// Run a `skill create` the server is expected to refuse, and return its
/// error output.
///
/// Should the server accept it after all, the new skill is deleted before the
/// test fails.
fn expect_create_refused(home: &Path, args: &[&str]) -> String {
    let output = run(home, args);
    if output.status.success() {
        let stdout = String::from_utf8_lossy(&output.stdout);
        if let Some(id) = serde_json::from_str::<Value>(&stdout)
            .ok()
            .and_then(|created| created["id"].as_str().map(str::to_string))
        {
            let _ = run(home, &["skill", "delete", id.as_str()]);
        }
        panic!(
            "memorylake {} unexpectedly succeeded (skill removed again):\n{stdout}",
            args.join(" ")
        );
    }
    assert_failure(&output, args)
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

    // Upload the next version's package separately; the printed URI serves
    // both the duplicate-name check and the version publish below.
    let (dir2, package2) = scratch_skill_package(&name, "When asked to greet, reply with hi.");
    let args = ["skill", "upload", package2.as_str()];
    let uploaded = parse_json(&assert_success(&run(&home, &args), &args), "upload");
    let uri = uploaded["s3_uri"].as_str().expect("upload prints s3_uri");

    // A second skill with the same name is refused (the spec says names need
    // not be unique; production disagrees).
    let args = [
        "skill",
        "create",
        "--name",
        name.as_str(),
        "--title",
        "duplicate",
        "--package-uri",
        uri,
    ];
    let err = expect_create_refused(&home, &args);
    assert!(err.contains("SKILL_NAME_CONFLICT"), "{err}");

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
    let err = expect_create_refused(&home, &args);
    assert!(err.contains("SKILL.md"), "{err}");
    assert!(err.contains("INVALID_ARGUMENT"), "{err}");
    // The upload went through before the refusal, so the error says how to
    // retry without uploading again.
    assert!(err.contains("--package-uri"), "{err}");

    let _ = fs::remove_dir_all(&dir);
    let _ = fs::remove_dir_all(&home);
}
