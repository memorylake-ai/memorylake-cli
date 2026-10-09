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
struct ScratchProject {
    home: PathBuf,
    workspace: String,
    id: String,
}

impl ScratchProject {
    fn create(home: PathBuf, workspace: String) -> Self {
        let name = unique_name("dbmem-proj");
        let args = [
            "project",
            "create",
            "--workspace",
            workspace.as_str(),
            "--name",
            name.as_str(),
            "--custom-id",
            name.as_str(),
        ];
        let stdout = assert_success(&run(&home, &args), &args);
        let created: Value = serde_json::from_str(&stdout).expect("project create JSON");
        let id = created["id"].as_str().expect("project id").to_string();
        Self {
            home,
            workspace,
            id,
        }
    }

    fn home(&self) -> &Path {
        &self.home
    }
}

impl Drop for ScratchProject {
    fn drop(&mut self) {
        let args = [
            "project",
            "delete",
            "--workspace",
            self.workspace.as_str(),
            self.id.as_str(),
        ];
        // Best effort: a failed cleanup must not mask the test's own result.
        let _ = run(&self.home, &args);
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
    let (ws, proj) = (workspace.as_str(), project.id.as_str());

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
