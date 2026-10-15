//! Installing, updating, inspecting and removing the bundled skills.

use std::fs;
use std::path::{Path, PathBuf};
use std::process::Output;

use serde_json::Value;

use crate::common::{assert_success, bin, temp_home};

/// The entry skill, which every build bundles.
const SKILL: &str = "memorylake-cli";

/// Run `memorylake args` with `home` as both the CLI state directory and the
/// home directory skills are installed under, from `cwd`.
fn run_in(home: &Path, cwd: &Path, args: &[&str]) -> Output {
    bin()
        .current_dir(cwd)
        .env("MEMORYLAKE_CONFIG_DIR", home.join(".memorylake"))
        .env("MEMORYLAKE_AGENT_SKILLS_HOME", home)
        .env("HOME", home)
        .env("USERPROFILE", home)
        .args(args)
        .output()
        .unwrap_or_else(|err| panic!("spawn memorylake {}: {err}", args.join(" ")))
}

fn run(home: &Path, args: &[&str]) -> Output {
    run_in(home, home, args)
}

fn json(home: &Path, args: &[&str]) -> Value {
    let stdout = assert_success(&run(home, args), args);
    serde_json::from_str(&stdout).unwrap_or_else(|err| panic!("{err}: {stdout}"))
}

/// The `action` of every result for `skill`, in order.
fn actions(report: &Value, skill: &str) -> Vec<String> {
    report["results"]
        .as_array()
        .expect("results")
        .iter()
        .filter(|result| result["skill"] == skill)
        .map(|result| result["action"].as_str().expect("action").to_string())
        .collect()
}

fn bundled_skill_md() -> String {
    let path =
        PathBuf::from(env!("CARGO_MANIFEST_DIR")).join(format!("../../skills/{SKILL}/SKILL.md"));
    fs::read_to_string(path).expect("read the bundled SKILL.md")
}

fn claude_dir(home: &Path) -> PathBuf {
    home.join(".claude").join("skills").join(SKILL)
}

fn agents_dir(home: &Path) -> PathBuf {
    home.join(".agents").join("skills").join(SKILL)
}

#[test]
fn installs_nothing_when_no_agent_is_found() {
    // An installer runs this on every machine; one without any coding agent
    // must not grow `.claude` or `.agents` directories for agents it lacks.
    let home = temp_home();
    let args = ["agent-skills", "install"];
    let output = run(&home, &args);
    let stdout = assert_success(&output, &args);
    let report: Value = serde_json::from_str(&stdout).expect("json");
    assert_eq!(report["results"], Value::Array(Vec::new()));
    assert!(String::from_utf8_lossy(&output.stderr).contains("no coding agent found"));
    assert!(!home.join(".claude").exists());
    assert!(!home.join(".agents").exists());
    let _ = fs::remove_dir_all(&home);
}

#[test]
fn installs_only_into_detected_agents() {
    let home = temp_home();
    fs::create_dir_all(home.join(".claude")).expect("create .claude");

    let report = json(&home, &["agent-skills", "install"]);
    assert_eq!(actions(&report, SKILL), ["installed"]);
    assert_eq!(
        fs::read_to_string(claude_dir(&home).join("SKILL.md")).expect("installed SKILL.md"),
        bundled_skill_md(),
        "the installed skill is the bundled one, byte for byte"
    );
    assert!(claude_dir(&home).join(".memorylake-skill.toml").is_file());
    assert!(
        !home.join(".agents").exists(),
        "no Agents-standard agent was found"
    );
    let _ = fs::remove_dir_all(&home);
}

#[test]
fn codex_gemini_and_opencode_select_the_agents_directory() {
    for marker in [".codex", ".gemini", ".config/opencode", ".agents"] {
        let home = temp_home();
        fs::create_dir_all(home.join(marker)).expect("create agent config dir");
        let report = json(&home, &["agent-skills", "install"]);
        assert_eq!(actions(&report, SKILL), ["installed"], "{marker}");
        assert!(agents_dir(&home).join("SKILL.md").is_file(), "{marker}");
        assert!(!home.join(".claude").exists(), "{marker}");
        let _ = fs::remove_dir_all(&home);
    }
}

#[test]
fn reinstalling_leaves_a_current_skill_alone() {
    let home = temp_home();
    json(&home, &["agent-skills", "install", "--target", "claude"]);
    let report = json(&home, &["agent-skills", "install", "--target", "claude"]);
    assert_eq!(actions(&report, SKILL), ["unchanged"]);
    let _ = fs::remove_dir_all(&home);
}

#[test]
fn an_edited_skill_is_kept_unless_forced() {
    // Someone who tailored the skill must not lose that work to an upgrade.
    let home = temp_home();
    json(&home, &["agent-skills", "install", "--target", "claude"]);
    let skill_md = claude_dir(&home).join("SKILL.md");
    fs::write(&skill_md, "my own notes").expect("edit SKILL.md");

    let report = json(&home, &["agent-skills", "install", "--target", "claude"]);
    assert_eq!(actions(&report, SKILL), ["skipped"]);
    assert_eq!(fs::read_to_string(&skill_md).expect("read"), "my own notes");

    let status = json(&home, &["agent-skills", "status", "--target", "claude"]);
    assert_eq!(status["skills"][0]["state"], "modified");

    let report = json(
        &home,
        &["agent-skills", "install", "--target", "claude", "--force"],
    );
    assert_eq!(actions(&report, SKILL), ["updated"]);
    assert_eq!(
        fs::read_to_string(&skill_md).expect("read"),
        bundled_skill_md()
    );
    let _ = fs::remove_dir_all(&home);
}

#[test]
fn a_file_added_to_an_installed_skill_counts_as_an_edit() {
    let home = temp_home();
    json(&home, &["agent-skills", "install", "--target", "claude"]);
    fs::write(claude_dir(&home).join("notes.md"), "mine").expect("add a file");
    // Files an operating system drops into a folder are not edits.
    fs::write(claude_dir(&home).join(".DS_Store"), "finder").expect("add .DS_Store");

    let status = json(&home, &["agent-skills", "status", "--target", "claude"]);
    assert_eq!(status["skills"][0]["state"], "modified");

    fs::remove_file(claude_dir(&home).join("notes.md")).expect("remove the file");
    let status = json(&home, &["agent-skills", "status", "--target", "claude"]);
    assert_eq!(status["skills"][0]["state"], "current");
    let _ = fs::remove_dir_all(&home);
}

#[test]
fn a_skill_it_did_not_install_is_never_touched() {
    // Even `--force` only overrides edits to skills this command wrote.
    let home = temp_home();
    let dir = agents_dir(&home);
    fs::create_dir_all(&dir).expect("create a foreign skill");
    fs::write(dir.join("SKILL.md"), "someone else's").expect("write a foreign skill");

    for args in [
        ["agent-skills", "install", "--target", "agents"].as_slice(),
        ["agent-skills", "install", "--target", "agents", "--force"].as_slice(),
    ] {
        let report = json(&home, args);
        assert_eq!(actions(&report, SKILL), ["skipped"]);
    }
    let status = json(&home, &["agent-skills", "status", "--target", "agents"]);
    assert_eq!(status["skills"][0]["state"], "foreign");

    json(&home, &["agent-skills", "uninstall", "--force"]);
    assert_eq!(
        fs::read_to_string(dir.join("SKILL.md")).expect("still there"),
        "someone else's"
    );
    let _ = fs::remove_dir_all(&home);
}

#[test]
fn a_skill_no_longer_bundled_is_removed_on_install() {
    // A later release can drop or rename a skill; the copy an earlier one
    // installed must not linger and keep steering agents at stale commands.
    let home = temp_home();
    json(&home, &["agent-skills", "install", "--target", "claude"]);
    let retired = home
        .join(".claude")
        .join("skills")
        .join("memorylake-retired");
    copy_dir(&claude_dir(&home), &retired);

    let status = json(&home, &["agent-skills", "status", "--target", "claude"]);
    let states: Vec<_> = status["skills"]
        .as_array()
        .expect("skills")
        .iter()
        .map(|entry| {
            (
                entry["skill"].as_str().unwrap().to_string(),
                entry["state"].clone(),
            )
        })
        .collect();
    assert!(states.contains(&("memorylake-retired".to_string(), Value::from("retired"))));

    let report = json(&home, &["agent-skills", "install", "--target", "claude"]);
    assert_eq!(actions(&report, "memorylake-retired"), ["removed"]);
    assert!(!retired.exists());
    let _ = fs::remove_dir_all(&home);
}

#[test]
fn uninstall_removes_what_it_installed_and_keeps_edits() {
    let home = temp_home();
    json(&home, &["agent-skills", "install", "--target", "all"]);
    fs::write(agents_dir(&home).join("SKILL.md"), "edited").expect("edit");

    let report = json(&home, &["agent-skills", "uninstall"]);
    let mut got = actions(&report, SKILL);
    got.sort();
    assert_eq!(got, ["removed", "skipped"]);
    assert!(!claude_dir(&home).exists());
    assert!(agents_dir(&home).exists(), "the edited copy is kept");

    let report = json(&home, &["agent-skills", "uninstall", "--force"]);
    assert_eq!(actions(&report, SKILL), ["removed"]);
    assert!(!agents_dir(&home).exists());
    let _ = fs::remove_dir_all(&home);
}

#[test]
fn project_installs_into_the_current_directory() {
    let home = temp_home();
    let project = home.join("repo");
    fs::create_dir_all(&project).expect("create project");

    let args = ["agent-skills", "install", "--project"];
    assert_success(&run_in(&home, &project, &args), &args);
    assert!(
        project
            .join(".claude/skills")
            .join(SKILL)
            .join("SKILL.md")
            .is_file()
    );
    assert!(
        project
            .join(".agents/skills")
            .join(SKILL)
            .join("SKILL.md")
            .is_file()
    );
    assert!(
        !home.join(".claude").exists(),
        "the home directory is not touched"
    );
    let _ = fs::remove_dir_all(&home);
}

#[test]
fn list_names_the_bundled_skills_and_targets() {
    let home = temp_home();
    fs::create_dir_all(home.join(".codex")).expect("create .codex");
    let listing = json(&home, &["agent-skills", "list"]);
    let skills = listing["skills"].as_array().expect("skills");
    assert!(skills.iter().any(|skill| skill["name"] == SKILL));
    assert!(skills.iter().all(|skill| skill["description"].is_string()));
    let targets = listing["targets"].as_array().expect("targets");
    let detected: Vec<_> = targets
        .iter()
        .map(|target| (target["target"].clone(), target["detected"].clone()))
        .collect();
    assert_eq!(
        detected,
        [
            (Value::from("claude"), Value::from(false)),
            (Value::from("agents"), Value::from(true))
        ]
    );
    let _ = fs::remove_dir_all(&home);
}

fn copy_dir(from: &Path, to: &Path) {
    fs::create_dir_all(to).expect("create copy");
    for entry in fs::read_dir(from).expect("read") {
        let entry = entry.expect("entry");
        let path = entry.path();
        if path.is_dir() {
            copy_dir(&path, &to.join(entry.file_name()));
        } else {
            fs::copy(&path, to.join(entry.file_name())).expect("copy");
        }
    }
}
