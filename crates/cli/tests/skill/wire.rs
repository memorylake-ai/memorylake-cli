//! Wire-level tests for `skill` against a loopback stub.
//!
//! Publishing from a local archive is three requests — ask for an upload
//! slot, PUT the archive to the slot's pre-signed URL, then create the skill
//! (or version) from the slot's storage URI. The stub hands out an upload URL
//! that points back at itself, so all three are pinned here.

use std::fs;
use std::path::PathBuf;

use serde_json::Value;

use crate::common::stub::{
    exchange, exchange_sequence, request_body, request_header, request_line,
};
use crate::common::{assert_failure, assert_success, unique_name};

const SKILL: &str = r#"{"success":true,"data":{"id":"skill-1","name":"equity","display_title":"Equity","latest_version":1,"latest_security_status":"pending","future_field":"kept"}}"#;
const SAFE_SKILL: &str = r#"{"success":true,"data":{"id":"skill-1","name":"equity","latest_version":2,"latest_security_status":"safe"}}"#;
const VERSION: &str = r#"{"success":true,"data":{"version":2,"skill_id":"skill-1","changelog":"v2","security_status":"pending"}}"#;
const EMPTY_PAGE: &str = r#"{"success":true,"data":{"items":[]}}"#;
const SLOT: &str = r#"{"success":true,"data":{"upload_url":"{base_url}/bucket/tmp/k.zip?X-Amz-Signature=sig","upload_headers":{"Content-Type":"application/zip"},"s3_uri":"s3://bucket/tmp/k.zip","expires_in_seconds":1800}}"#;
const STORAGE_OK: &str = "";
const PACKAGE: &[u8] = b"PK\x03\x04fake-archive-bytes";

fn body_json(request: &str) -> Value {
    serde_json::from_str(request_body(request)).expect("request body is JSON")
}

fn stdout_json(output: &std::process::Output) -> Value {
    serde_json::from_slice(&output.stdout).expect("stdout is JSON")
}

/// A scratch `skill.zip` holding [`PACKAGE`].
fn scratch_package() -> (PathBuf, String) {
    let dir = std::env::temp_dir().join(unique_name("skill-wire"));
    fs::create_dir_all(&dir).expect("create scratch dir");
    let path = dir.join("skill.zip");
    fs::write(&path, PACKAGE).expect("write package");
    let arg = path.to_string_lossy().into_owned();
    (dir, arg)
}

/// Pin the slot request and the storage PUT that follow `--package`.
fn assert_uploaded(slot: &str, put: &str) {
    assert_eq!(
        request_line(slot),
        "POST /api/v3/skills/package-uploads HTTP/1.1"
    );
    assert_eq!(
        request_line(put),
        "PUT /bucket/tmp/k.zip?X-Amz-Signature=sig HTTP/1.1"
    );
    assert_eq!(request_header(put, "content-type"), Some("application/zip"));
    assert!(
        request_header(put, "authorization").is_none(),
        "the MemoryLake key must not reach storage:\n{put}"
    );
    assert_eq!(request_body(put).as_bytes(), PACKAGE);
}

#[test]
fn list_sends_paging_and_the_name_filter() {
    let args = [
        "skill",
        "list",
        "--page-size",
        "5",
        "--continuation-token",
        "tok",
        "--name",
        "equity",
    ];
    let (request, output) = exchange(EMPTY_PAGE, &args);
    assert_success(&output, &args);
    assert_eq!(
        request_line(&request),
        "GET /api/v3/skills?page_size=5&continuation_token=tok&name_fuzzy=equity HTTP/1.1"
    );
}

#[test]
fn create_with_a_package_uploads_then_creates() {
    let (dir, package) = scratch_package();
    let args = [
        "skill",
        "create",
        "--name",
        "equity",
        "--title",
        "Equity",
        "--description",
        "Reads filings",
        "--package",
        package.as_str(),
    ];
    let (requests, output) = exchange_sequence(&[SLOT, STORAGE_OK, SKILL], &args);
    assert_success(&output, &args);

    assert_uploaded(&requests[0], &requests[1]);
    assert_eq!(request_line(&requests[2]), "POST /api/v3/skills HTTP/1.1");
    assert_eq!(
        body_json(&requests[2]),
        serde_json::json!({
            "name": "equity",
            "display_title": "Equity",
            "description": "Reads filings",
            "package_ref": {"s3_uri": "s3://bucket/tmp/k.zip"}
        })
    );

    let printed = stdout_json(&output);
    assert_eq!(printed["id"], "skill-1");
    assert_eq!(printed["future_field"], "kept", "unmodeled fields survive");
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(stderr.contains("review pending"), "{stderr}");
    assert!(stderr.contains("skill get skill-1"), "{stderr}");
    let _ = fs::remove_dir_all(&dir);
}

#[test]
fn create_with_a_package_uri_skips_the_upload() {
    let args = [
        "skill",
        "create",
        "--name",
        "equity",
        "--title",
        "Equity",
        "--package-uri",
        "s3://bucket/tmp/earlier.zip",
    ];
    let (request, output) = exchange(SAFE_SKILL, &args);
    assert_success(&output, &args);
    assert_eq!(request_line(&request), "POST /api/v3/skills HTTP/1.1");
    assert_eq!(
        body_json(&request),
        serde_json::json!({
            "name": "equity",
            "display_title": "Equity",
            "package_ref": {"s3_uri": "s3://bucket/tmp/earlier.zip"}
        })
    );
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        stderr.is_empty(),
        "no hint once the review is done: {stderr}"
    );
}

#[test]
fn upload_prints_the_storage_uri() {
    let (dir, package) = scratch_package();
    let args = ["skill", "upload", package.as_str()];
    let (requests, output) = exchange_sequence(&[SLOT, STORAGE_OK], &args);
    assert_success(&output, &args);

    assert_uploaded(&requests[0], &requests[1]);
    assert_eq!(
        stdout_json(&output),
        serde_json::json!({"s3_uri": "s3://bucket/tmp/k.zip"})
    );
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(
        !stdout.contains("X-Amz-Signature"),
        "the signed URL is a credential and must not be printed: {stdout}"
    );
    let _ = fs::remove_dir_all(&dir);
}

#[test]
fn get_update_and_delete_address_the_skill() {
    let args = ["skill", "get", "skill-1"];
    let (request, output) = exchange(SAFE_SKILL, &args);
    assert_success(&output, &args);
    assert_eq!(
        request_line(&request),
        "GET /api/v3/skills/skill-1 HTTP/1.1"
    );

    let args = [
        "skill",
        "update",
        "skill-1",
        "--title",
        "New",
        "--description",
        "",
    ];
    let (request, output) = exchange(SAFE_SKILL, &args);
    assert_success(&output, &args);
    assert_eq!(
        request_line(&request),
        "PATCH /api/v3/skills/skill-1 HTTP/1.1"
    );
    assert_eq!(
        body_json(&request),
        serde_json::json!({"display_title": "New", "description": ""})
    );

    let args = ["skill", "delete", "skill-1"];
    let (request, output) = exchange(r#"{"success":true}"#, &args);
    let stdout = assert_success(&output, &args);
    assert_eq!(
        request_line(&request),
        "DELETE /api/v3/skills/skill-1 HTTP/1.1"
    );
    assert!(stdout.contains("Deleted skill `skill-1`"), "{stdout}");
}

#[test]
fn version_create_uploads_then_publishes() {
    let (dir, package) = scratch_package();
    let args = [
        "skill",
        "version",
        "create",
        "skill-1",
        "--changelog",
        "v2",
        "--package",
        package.as_str(),
    ];
    let (requests, output) = exchange_sequence(&[SLOT, STORAGE_OK, VERSION], &args);
    assert_success(&output, &args);

    assert_uploaded(&requests[0], &requests[1]);
    assert_eq!(
        request_line(&requests[2]),
        "POST /api/v3/skills/skill-1/versions HTTP/1.1"
    );
    assert_eq!(
        body_json(&requests[2]),
        serde_json::json!({
            "changelog": "v2",
            "package_ref": {"s3_uri": "s3://bucket/tmp/k.zip"}
        })
    );
    assert_eq!(stdout_json(&output)["version"], 2);
    let _ = fs::remove_dir_all(&dir);
}

#[test]
fn version_list_and_get_address_the_version() {
    let args = ["skill", "version", "list", "skill-1", "--page-size", "2"];
    let (request, output) = exchange(
        r#"{"success":true,"data":{"items":[{"version":1}],"total":1}}"#,
        &args,
    );
    assert_success(&output, &args);
    assert_eq!(
        request_line(&request),
        "GET /api/v3/skills/skill-1/versions?page_size=2 HTTP/1.1"
    );
    assert_eq!(stdout_json(&output)["total"], 1);

    let args = ["skill", "version", "get", "skill-1", "2"];
    let (request, output) = exchange(VERSION, &args);
    assert_success(&output, &args);
    assert_eq!(
        request_line(&request),
        "GET /api/v3/skills/skill-1/versions/2 HTTP/1.1"
    );
}

#[test]
fn a_name_conflict_surfaces_the_error_code() {
    let args = [
        "skill",
        "create",
        "--name",
        "equity",
        "--title",
        "Equity",
        "--package-uri",
        "s3://bucket/tmp/k.zip",
    ];
    let (_, output) = exchange(
        r#"{"success":false,"message":"A skill named 'equity' already exists.","error_code":"SKILL_NAME_CONFLICT"}"#,
        &args,
    );
    let err = assert_failure(&output, &args);
    assert!(err.contains("SKILL_NAME_CONFLICT"), "{err}");
}

#[test]
fn a_failed_create_after_upload_names_the_uri_to_retry_with() {
    let (dir, package) = scratch_package();
    let args = [
        "skill",
        "create",
        "--name",
        "equity",
        "--title",
        "Equity",
        "--package",
        package.as_str(),
    ];
    let conflict = r#"{"success":false,"message":"A skill named 'equity' already exists.","error_code":"SKILL_NAME_CONFLICT"}"#;
    let (_, output) = exchange_sequence(&[SLOT, STORAGE_OK, conflict], &args);
    let err = assert_failure(&output, &args);
    assert!(err.contains("SKILL_NAME_CONFLICT"), "{err}");
    assert!(
        err.contains("--package-uri s3://bucket/tmp/k.zip"),
        "the error must say how to retry without re-uploading: {err}"
    );
    assert!(!err.contains("X-Amz-Signature"), "{err}");
    let _ = fs::remove_dir_all(&dir);
}

#[test]
fn a_failed_create_from_a_uri_adds_no_retry_hint() {
    let args = [
        "skill",
        "create",
        "--name",
        "equity",
        "--title",
        "Equity",
        "--package-uri",
        "s3://bucket/tmp/k.zip",
    ];
    let (_, output) = exchange(
        r#"{"success":false,"message":"nope","error_code":"INVALID_ARGUMENT"}"#,
        &args,
    );
    let err = assert_failure(&output, &args);
    assert!(!err.contains("without uploading again"), "{err}");
}
