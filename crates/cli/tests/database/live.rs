//! Live `project database` tests (require `MEMORYLAKE_API_KEY`).
//!
//! A database memory needs a datasource, which needs a working database
//! connection — not available on production (2026-10-09). So these cover the
//! read path on an empty project and the error paths only. The project is
//! created in the account's default workspace and removed again by a guard.

use std::fs;
use std::path::{Path, PathBuf};

use serde_json::Value;

use crate::common::{assert_failure, assert_success, require_api_key, run, temp_home, unique_name};
use crate::datasource::live::{default_workspace, login_default};

/// A scratch project that deletes itself, and the temp `$HOME`, on drop.
///
/// The project is known by the unique `custom_id` it is created with, so the
/// guard exists before `project create` runs: a panic anywhere after that —
/// even while reading the create response — still finds and deletes it.
struct ScratchProject {
    home: PathBuf,
    workspace: String,
    custom_id: String,
    id: Option<String>,
}

impl ScratchProject {
    fn create(home: PathBuf, workspace: String) -> Self {
        let mut project = Self {
            home,
            workspace,
            custom_id: unique_name("dbmem-proj"),
            id: None,
        };
        let args = [
            "project",
            "create",
            "--workspace",
            project.workspace.as_str(),
            "--name",
            project.custom_id.as_str(),
            "--custom-id",
            project.custom_id.as_str(),
        ];
        let stdout = assert_success(&run(&project.home, &args), &args);
        let created: Value = serde_json::from_str(&stdout).expect("project create JSON");
        project.id = Some(created["id"].as_str().expect("project id").to_string());
        project
    }

    fn home(&self) -> &Path {
        &self.home
    }

    fn id(&self) -> &str {
        self.id.as_deref().expect("project created")
    }

    /// The project's id, looked up by custom_id when create did not get as
    /// far as reporting it.
    fn find_id(&self) -> Option<String> {
        if let Some(id) = &self.id {
            return Some(id.clone());
        }
        let args = [
            "project",
            "get",
            "--workspace",
            self.workspace.as_str(),
            "--by-custom-id",
            self.custom_id.as_str(),
        ];
        let output = run(&self.home, &args);
        if !output.status.success() {
            return None;
        }
        let found: Value = serde_json::from_slice(&output.stdout).ok()?;
        found["id"].as_str().map(str::to_string)
    }
}

impl Drop for ScratchProject {
    fn drop(&mut self) {
        // Best effort: a failed cleanup must not mask the test's own result.
        // Production drops the odd request, so try a few times.
        for _ in 0..3 {
            let Some(id) = self.find_id() else { continue };
            let args = [
                "project",
                "delete",
                "--workspace",
                self.workspace.as_str(),
                id.as_str(),
            ];
            if run(&self.home, &args).status.success() {
                break;
            }
        }
        let _ = fs::remove_dir_all(&self.home);
    }
}

#[test]
fn database_memory_reads_and_error_paths() {
    let api_key = require_api_key();
    let home = temp_home();
    login_default(&home, &api_key);
    let workspace = default_workspace(&home);
    let project = ScratchProject::create(home, workspace.clone());
    let (ws, proj) = (workspace.as_str(), project.id());

    // An empty project lists nothing; production includes `total` here.
    let args = [
        "project",
        "database",
        "list",
        "--workspace",
        ws,
        "--project",
        proj,
    ];
    let stdout = assert_success(&run(project.home(), &args), &args);
    let page: Value = serde_json::from_str(&stdout).expect("list JSON");
    assert_eq!(page["items"], Value::Array(vec![]), "{stdout}");

    for args in [
        vec![
            "project",
            "database",
            "get",
            "--workspace",
            ws,
            "--project",
            proj,
            "mlcli-no-such-db",
        ],
        vec![
            "project",
            "database",
            "generate-instruction",
            "--workspace",
            ws,
            "--project",
            proj,
            "mlcli-no-such-db",
        ],
        vec![
            "project",
            "database",
            "reload",
            "--workspace",
            ws,
            "--project",
            proj,
            "mlcli-no-such-db",
        ],
        // Fails on the unknown datasource, so nothing is created.
        vec![
            "project",
            "database",
            "create",
            "--workspace",
            ws,
            "--project",
            proj,
            "--datasource",
            "mlcli-no-such-ds",
            "--name",
            "mlcli-probe",
        ],
    ] {
        let output = assert_failure(&run(project.home(), &args), &args);
        assert!(output.contains("NOT_FOUND"), "{args:?}: {output}");
    }
}
