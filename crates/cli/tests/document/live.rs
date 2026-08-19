//! Live `project document` tests (require `MEMORYLAKE_API_KEY`).
//!
//! Each test builds its own scratch Library folder and project and removes both
//! on the normal path. Cleanup is deliberately not run on failure so the state
//! can be inspected. The workspace itself is left behind: `workspace delete` is
//! not implemented.

use std::fs;
use std::path::Path;
use std::time::{SystemTime, UNIX_EPOCH};

use serde_json::Value;

use crate::common::{
    assert_failure, assert_success, live_base_url, login_args, require_api_key, run,
    scratch_text_file, temp_home, unique_name,
};

/// Seconds to let the server finish indexing under `--wait`.
///
/// A timeout here is a real failure: either indexing a few KB of text took
/// longer than five minutes, or the polling logic is not seeing terminal
/// statuses.
const WAIT_TIMEOUT_SECS: &str = "300";

fn nanos() -> u128 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("clock")
        .as_nanos()
}

fn login(home: &Path, api_key: &str) {
    let base_url = live_base_url();
    let args = login_args(api_key, "default", base_url.as_deref());
    assert_success(&run(home, &args), &args);
}

fn json(stdout: &str) -> Value {
    serde_json::from_str(stdout).unwrap_or_else(|err| panic!("parse JSON output: {err}\n{stdout}"))
}

fn field<'a>(value: &'a Value, key: &str) -> &'a str {
    value
        .get(key)
        .and_then(Value::as_str)
        .unwrap_or_else(|| panic!("expected a string `{key}` in {value}"))
}

/// Create a workspace to host the project under test and return its id.
fn create_workspace(home: &Path, label: &str) -> String {
    let name = format!("CLI Live {label} {}", nanos());
    let custom_id = format!("cli-live-{label}-{}", nanos());
    let args = [
        "workspace",
        "create",
        "--name",
        name.as_str(),
        "--custom-id",
        custom_id.as_str(),
    ];
    let created = json(&assert_success(&run(home, &args), &args));
    field(&created, "id").to_string()
}

/// Create a project inside `workspace` and return its id.
fn create_project(home: &Path, workspace: &str, label: &str) -> String {
    let name = format!("CLI Docs {label} {}", nanos());
    let custom_id = format!("cli-docs-{label}-{}", nanos());
    let args = [
        "project",
        "create",
        "--workspace",
        workspace,
        "--name",
        name.as_str(),
        "--custom-id",
        custom_id.as_str(),
    ];
    let created = json(&assert_success(&run(home, &args), &args));
    field(&created, "id").to_string()
}

fn delete_project(home: &Path, workspace: &str, project: &str) {
    let args = ["project", "delete", "--workspace", workspace, project];
    assert_success(&run(home, &args), &args);
}

/// Create a scratch Library folder under the workspace root and return its id.
fn make_scratch_folder(home: &Path, tag: &str) -> String {
    let name = unique_name(tag);
    let args = ["library", "mkdir", name.as_str(), "--on-conflict", "deny"];
    let created = json(&assert_success(&run(home, &args), &args));
    field(&created, "item_id").to_string()
}

fn delete_scratch_folder(home: &Path, item_id: &str) {
    let args = ["library", "delete", item_id];
    assert_success(&run(home, &args), &args);
}

/// Upload a small file into `parent` and return its Library item id.
fn upload_file(home: &Path, parent: &str, tag: &str, name: &str) -> String {
    let (dir, path) = scratch_text_file(tag);
    let source = path.to_str().expect("utf-8 scratch path");
    let args = ["lib", "upload", source, "--parent", parent, "--name", name];
    let uploaded = json(&assert_success(&run(home, &args), &args));
    let item_id = field(&uploaded, "item_id").to_string();
    let _ = fs::remove_dir_all(&dir);
    item_id
}

#[test]
fn document_lifecycle_import_wait_list_get_delete() {
    let api_key = require_api_key();
    let home = temp_home();
    login(&home, &api_key);

    let workspace = create_workspace(&home, "docs");
    let project = create_project(&home, &workspace, "lifecycle");
    let folder = make_scratch_folder(&home, "cli-docs-lifecycle");
    let file_id = upload_file(&home, &folder, "cli-docs-lifecycle", "lifecycle.txt");

    // Import, waiting for indexing to finish. A non-zero exit here means either
    // a file failed or a document ended in `error`; both must fail the test.
    let args = [
        "project",
        "document",
        "import",
        "--workspace",
        workspace.as_str(),
        "--project",
        project.as_str(),
        file_id.as_str(),
        "--wait",
        "--timeout",
        WAIT_TIMEOUT_SECS,
    ];
    let outcome = json(&assert_success(&run(&home, &args), &args));
    assert_eq!(
        outcome.get("success_count").and_then(Value::as_u64),
        Some(1),
        "expected exactly one imported file: {outcome}"
    );
    assert_eq!(
        outcome.get("failure_count").and_then(Value::as_u64),
        Some(0),
        "import reported a failure: {outcome}"
    );

    // `--wait` is only meaningful if the response actually names the documents
    // to poll. If the API stops returning `document_id`, waiting would silently
    // become a no-op, so assert the id is there.
    let details = outcome
        .get("details")
        .and_then(Value::as_array)
        .expect("import response has details");
    assert_eq!(details.len(), 1, "one file in, one detail out: {outcome}");
    let document_id = field(&details[0], "document_id").to_string();
    assert_eq!(field(&details[0], "result"), "success");

    // The document must be visible in the project listing.
    let args = [
        "proj",
        "doc",
        "list",
        "--workspace",
        workspace.as_str(),
        "--project",
        project.as_str(),
        "--page-size",
        "50",
    ];
    let listed = json(&assert_success(&run(&home, &args), &args));
    let items = listed
        .get("items")
        .and_then(Value::as_array)
        .expect("list response has items");
    assert!(
        items
            .iter()
            .any(|item| item.get("id").and_then(Value::as_str) == Some(document_id.as_str())),
        "imported document {document_id} missing from the listing: {listed}"
    );

    // Get, which is also the call `--wait` polls with.
    let args = [
        "project",
        "document",
        "get",
        "--workspace",
        workspace.as_str(),
        "--project",
        project.as_str(),
        document_id.as_str(),
    ];
    let fetched = json(&assert_success(&run(&home, &args), &args));
    assert_eq!(field(&fetched, "id"), document_id);
    assert_eq!(
        field(&fetched, "status"),
        "okay",
        "--wait returned but the document is not in a terminal okay state: {fetched}"
    );
    assert_eq!(field(&fetched, "document_type"), "drive_file");

    // Delete, then prove the document is really gone.
    let args = [
        "project",
        "document",
        "delete",
        "--workspace",
        workspace.as_str(),
        "--project",
        project.as_str(),
        document_id.as_str(),
    ];
    let stdout = assert_success(&run(&home, &args), &args);
    assert!(
        stdout.contains(&document_id) && stdout.contains("Deleted 1 document(s)"),
        "unexpected delete output: {stdout}"
    );

    let args = [
        "project",
        "document",
        "get",
        "--workspace",
        workspace.as_str(),
        "--project",
        project.as_str(),
        document_id.as_str(),
    ];
    let err = assert_failure(&run(&home, &args), &args);
    assert!(
        !err.is_empty(),
        "getting a deleted document must fail with a message"
    );

    // The Library file must have survived the document delete.
    let args = ["library", "get", file_id.as_str()];
    assert_success(&run(&home, &args), &args);

    delete_scratch_folder(&home, &folder);
    delete_project(&home, &workspace, &project);
    let _ = fs::remove_dir_all(&home);
}

#[test]
fn a_folder_without_recursive_is_refused_and_imports_nothing() {
    let api_key = require_api_key();
    let home = temp_home();
    login(&home, &api_key);

    let workspace = create_workspace(&home, "docsguard");
    let project = create_project(&home, &workspace, "guard");
    let folder = make_scratch_folder(&home, "cli-docs-guard");
    upload_file(&home, &folder, "cli-docs-guard", "guarded.txt");

    let args = [
        "project",
        "document",
        "import",
        "--workspace",
        workspace.as_str(),
        "--project",
        project.as_str(),
        folder.as_str(),
    ];
    let err = assert_failure(&run(&home, &args), &args);
    assert!(
        err.contains("--recursive"),
        "a folder id must be refused with a pointer to --recursive: {err}"
    );

    // The refusal has to happen before the import request, so the project must
    // still hold nothing.
    let args = [
        "project",
        "document",
        "list",
        "--workspace",
        workspace.as_str(),
        "--project",
        project.as_str(),
    ];
    let listed = json(&assert_success(&run(&home, &args), &args));
    let items = listed
        .get("items")
        .and_then(Value::as_array)
        .expect("list response has items");
    assert!(
        items.is_empty(),
        "nothing may be imported when a folder is refused: {listed}"
    );

    delete_scratch_folder(&home, &folder);
    delete_project(&home, &workspace, &project);
    let _ = fs::remove_dir_all(&home);
}

#[test]
fn recursive_import_pulls_in_every_file_under_the_folder() {
    let api_key = require_api_key();
    let home = temp_home();
    login(&home, &api_key);

    let workspace = create_workspace(&home, "docsrecursive");
    let project = create_project(&home, &workspace, "recursive");
    let folder = make_scratch_folder(&home, "cli-docs-recursive");
    let first = upload_file(&home, &folder, "cli-docs-recursive", "one.txt");
    let second = upload_file(&home, &folder, "cli-docs-recursive", "two.txt");

    let args = [
        "proj",
        "doc",
        "import",
        "--workspace",
        workspace.as_str(),
        "--project",
        project.as_str(),
        folder.as_str(),
        "--recursive",
    ];
    let outcome = json(&assert_success(&run(&home, &args), &args));
    assert_eq!(
        outcome.get("success_count").and_then(Value::as_u64),
        Some(2),
        "both files under the folder should have been imported: {outcome}"
    );
    assert_eq!(
        outcome.get("failure_count").and_then(Value::as_u64),
        Some(0),
        "{outcome}"
    );

    // Both Library items must appear among the per-file details.
    let details = outcome
        .get("details")
        .and_then(Value::as_array)
        .expect("import response has details");
    for item_id in [first.as_str(), second.as_str()] {
        assert!(
            details
                .iter()
                .any(|d| d.get("drive_item_id").and_then(Value::as_str) == Some(item_id)),
            "expanded file {item_id} missing from import details: {outcome}"
        );
    }

    // Importing the same folder again must be reported as duplicates, not as a
    // second set of documents.
    let args = [
        "proj",
        "doc",
        "import",
        "--workspace",
        workspace.as_str(),
        "--project",
        project.as_str(),
        folder.as_str(),
        "--recursive",
    ];
    let repeat = json(&assert_success(&run(&home, &args), &args));
    assert_eq!(
        repeat.get("duplicate_count").and_then(Value::as_u64),
        Some(2),
        "re-importing the same files must be duplicates, and duplicates must exit zero: {repeat}"
    );

    // Clean up the documents this test created before removing the project.
    let document_ids: Vec<String> = details
        .iter()
        .filter_map(|d| d.get("document_id").and_then(Value::as_str))
        .map(str::to_string)
        .collect();
    assert_eq!(
        document_ids.len(),
        2,
        "expected two document ids: {outcome}"
    );
    let mut args = vec![
        "project",
        "document",
        "delete",
        "--workspace",
        workspace.as_str(),
        "--project",
        project.as_str(),
    ];
    args.extend(document_ids.iter().map(String::as_str));
    let stdout = assert_success(&run(&home, &args), &args);
    assert!(
        stdout.contains("Deleted 2 document(s)"),
        "batch delete must report both documents: {stdout}"
    );

    delete_scratch_folder(&home, &folder);
    delete_project(&home, &workspace, &project);
    let _ = fs::remove_dir_all(&home);
}

#[test]
fn download_round_trips_the_uploaded_bytes() {
    // The download endpoint answers 303 with a pre-signed storage URL rather
    // than the bytes, so the only way to know the whole chain works — redirect
    // followed, credentials not carried across, content intact — is to upload
    // known content and compare what comes back.
    let api_key = require_api_key();
    let home = temp_home();
    login(&home, &api_key);

    let workspace = create_workspace(&home, "dl");
    let project = create_project(&home, &workspace, "download");
    let folder = make_scratch_folder(&home, "cli-docs-download");

    let (source_dir, source_path) = scratch_text_file("cli-docs-download");
    let expected = fs::read(&source_path).expect("read the file being uploaded");
    let source = source_path.to_str().expect("utf-8 scratch path");
    let args = [
        "lib",
        "upload",
        source,
        "--parent",
        folder.as_str(),
        "--name",
        "download-me.txt",
    ];
    let uploaded = json(&assert_success(&run(&home, &args), &args));
    let item_id = field(&uploaded, "item_id").to_string();
    let _ = fs::remove_dir_all(&source_dir);

    let args = [
        "project",
        "document",
        "import",
        "--workspace",
        workspace.as_str(),
        "--project",
        project.as_str(),
        item_id.as_str(),
        "--wait",
    ];
    assert_success(&run(&home, &args), &args);

    let args = [
        "project",
        "document",
        "list",
        "--workspace",
        workspace.as_str(),
        "--project",
        project.as_str(),
    ];
    let listing = json(&assert_success(&run(&home, &args), &args));
    let document_id = listing["items"][0]["id"]
        .as_str()
        .expect("the imported document is listed")
        .to_string();

    // Download into a directory this test owns, under the name the server
    // reports — the path a caller gets when they pass no --output.
    let destination = home.join("downloads");
    fs::create_dir_all(&destination).expect("create download directory");
    let args = [
        "project",
        "document",
        "download",
        "--workspace",
        workspace.as_str(),
        "--project",
        project.as_str(),
        document_id.as_str(),
        "--output",
        destination.to_str().expect("utf-8 destination"),
    ];
    let stdout = assert_success(&run(&home, &args), &args);
    assert!(
        stdout.contains("download-me.txt"),
        "the server's name is used and reported: {stdout}"
    );

    let written = destination.join("download-me.txt");
    let actual = fs::read(&written).expect("the downloaded file exists");
    assert_eq!(
        actual, expected,
        "downloaded bytes differ from what was uploaded"
    );

    // A second download must not silently replace the first.
    let output = run(&home, &args);
    assert!(
        !output.status.success(),
        "an existing file must not be overwritten without --force"
    );

    let mut forced = args.to_vec();
    forced.push("--force");
    assert_success(&run(&home, &forced), &forced);
    assert_eq!(
        fs::read(&written).expect("read after --force"),
        expected,
        "--force rewrites the same content"
    );

    delete_scratch_folder(&home, &folder);
    delete_project(&home, &workspace, &project);
    let _ = fs::remove_dir_all(&home);
}
