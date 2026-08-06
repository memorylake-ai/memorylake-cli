//! Wire-level `actor` tests against a loopback stub of the MemoryLake API.
//!
//! These need no API key: they pin the HTTP method, path, query string, and
//! body that each subcommand sends, and the output it prints back. Live tests
//! prove the endpoints exist; these prove the CLI calls the documented ones.

use std::fs;
use std::io::{Read, Write};
use std::net::{TcpListener, TcpStream};
use std::path::PathBuf;
use std::process::Output;
use std::sync::mpsc::{Receiver, RecvTimeoutError, channel};
use std::thread::JoinHandle;
use std::time::Duration;

use crate::common::{assert_success, run, temp_home};

/// One-shot loopback HTTP server that answers with a canned envelope and hands
/// the raw request text back to the test.
struct StubServer {
    base_url: String,
    requests: Receiver<String>,
    handle: Option<JoinHandle<()>>,
}

impl StubServer {
    fn new(body: &str) -> Self {
        let listener = TcpListener::bind("127.0.0.1:0").expect("bind stub server");
        let addr = listener.local_addr().expect("stub server address");
        let response = format!(
            "HTTP/1.1 200 OK\r\ncontent-type: application/json\r\ncontent-length: {}\r\nconnection: close\r\n\r\n{body}",
            body.len()
        );

        let (sender, requests) = channel();
        let handle = std::thread::spawn(move || {
            let Ok((mut stream, _)) = listener.accept() else {
                return;
            };
            let request = read_http_request(&mut stream);
            // Best effort: a dropped receiver means the test already ended.
            let _ = sender.send(request);
            let _ = stream.write_all(response.as_bytes());
            let _ = stream.flush();
        });

        Self {
            base_url: format!("http://{addr}"),
            requests,
            handle: Some(handle),
        }
    }

    fn received(&self) -> String {
        match self.requests.recv_timeout(Duration::from_secs(10)) {
            Ok(request) => request,
            Err(RecvTimeoutError::Timeout) => panic!("stub server received no request"),
            Err(RecvTimeoutError::Disconnected) => panic!("stub server thread died"),
        }
    }
}

impl Drop for StubServer {
    fn drop(&mut self) {
        if let Some(handle) = self.handle.take() {
            // Best effort: a panicking server thread already failed the test.
            let _ = handle.join();
        }
    }
}

/// Read one complete HTTP request (head plus `content-length` body).
fn read_http_request(stream: &mut TcpStream) -> String {
    let mut data = Vec::new();
    let mut chunk = [0u8; 1024];
    loop {
        let read = match stream.read(&mut chunk) {
            Ok(0) | Err(_) => break,
            Ok(read) => read,
        };
        data.extend_from_slice(&chunk[..read]);

        let Some(head_end) = data.windows(4).position(|window| window == b"\r\n\r\n") else {
            continue;
        };
        let head = String::from_utf8_lossy(&data[..head_end]).to_ascii_lowercase();
        let content_length = head
            .lines()
            .find_map(|line| line.strip_prefix("content-length:"))
            .and_then(|value| value.trim().parse::<usize>().ok())
            .unwrap_or(0);
        if data.len() >= head_end + 4 + content_length {
            break;
        }
    }
    String::from_utf8_lossy(&data).into_owned()
}

/// A temp `$HOME` already holding credentials that point at `base_url`.
fn logged_in_home(base_url: &str) -> PathBuf {
    let home = temp_home();
    let root = home.join(".memorylake");
    fs::create_dir_all(&root).expect("create .memorylake");
    fs::write(
        root.join("config.toml"),
        format!("active_profile = \"default\"\n\n[profiles.default]\nbase_url = \"{base_url}\"\n"),
    )
    .expect("write config.toml");
    fs::write(
        root.join("credentials.toml"),
        "[profiles.default]\napi_key = \"sk_offline_stub_key\"\nlogin_method = \"api_key\"\n",
    )
    .expect("write credentials.toml");
    home
}

/// Run one command against a stub returning `response`, and report both the
/// request the CLI sent and the process output.
fn exchange(response: &str, args: &[&str]) -> (String, Output) {
    let server = StubServer::new(response);
    let home = logged_in_home(&server.base_url);
    let output = run(&home, args);
    let request = server.received();
    let _ = fs::remove_dir_all(&home);
    (request, output)
}

const EMPTY_PAGE: &str = r#"{"success":true,"data":{"items":[],"continuation_token":null}}"#;
const ONE_ACTOR: &str = r#"{"success":true,"data":{"id":"act-1","custom_id":"user-1","actor_type":"HUMAN","display_name":"Alice"}}"#;
const NO_DATA: &str = r#"{"success":true,"message":"Operation completed successfully"}"#;

fn request_line(request: &str) -> &str {
    request.lines().next().unwrap_or_default()
}

#[test]
fn list_calls_the_account_wide_endpoint() {
    let (request, output) = exchange(
        EMPTY_PAGE,
        &[
            "actor",
            "list",
            "--page-size",
            "2",
            "--continuation-token",
            "tok-1",
            "--type",
            "HUMAN",
            "--name",
            "Ali",
        ],
    );
    assert_success(&output, &["actor", "list"]);

    let line = request_line(&request);
    assert!(line.starts_with("GET /api/v3/actors?"), "{line}");
    for expected in [
        "page_size=2",
        "continuation_token=tok-1",
        "actor_type=HUMAN",
        "display_name_fuzzy=Ali",
    ] {
        assert!(line.contains(expected), "missing {expected} in {line}");
    }
}

#[test]
fn list_with_workspace_calls_the_workspace_scoped_endpoint() {
    let (request, output) = exchange(
        EMPTY_PAGE,
        &["actor", "list", "--workspace", "ws-1", "--page-size", "2"],
    );
    assert_success(&output, &["actor", "list", "--workspace"]);

    let line = request_line(&request);
    assert!(
        line.starts_with("GET /api/v3/workspaces/ws-1/actors?"),
        "{line}"
    );
    assert!(line.contains("page_size=2"), "{line}");
}

#[test]
fn create_posts_the_documented_body() {
    let (request, output) = exchange(
        ONE_ACTOR,
        &[
            "actor",
            "create",
            "--custom-id",
            "user-1",
            "--display-name",
            "Alice",
            "--type",
            "ASSISTANT",
            "--description",
            "intake bot",
            "--metadata",
            r#"{"tier":"premium"}"#,
        ],
    );
    assert_success(&output, &["actor", "create"]);

    assert!(
        request_line(&request).starts_with("POST /api/v3/actors "),
        "{}",
        request_line(&request)
    );
    for expected in [
        r#""custom_id":"user-1""#,
        r#""display_name":"Alice""#,
        r#""actor_type":"ASSISTANT""#,
        r#""description":"intake bot""#,
        r#""metadata":{"tier":"premium"}"#,
    ] {
        assert!(
            request.contains(expected),
            "missing {expected} in {request}"
        );
    }
}

#[test]
fn create_omits_flags_that_were_not_passed() {
    let (request, _) = exchange(
        ONE_ACTOR,
        &[
            "actor",
            "create",
            "--custom-id",
            "user-1",
            "--display-name",
            "Alice",
        ],
    );
    assert!(!request.contains("actor_type"), "{request}");
    assert!(!request.contains("description"), "{request}");
    assert!(!request.contains("metadata"), "{request}");
}

#[test]
fn get_uses_the_id_path_and_by_custom_id_query() {
    let (request, output) = exchange(ONE_ACTOR, &["actor", "get", "act-1"]);
    assert_success(&output, &["actor", "get"]);
    assert_eq!(
        request_line(&request).split(' ').nth(1),
        Some("/api/v3/actors/act-1")
    );
    assert!(request_line(&request).starts_with("GET "));

    let (request, output) = exchange(ONE_ACTOR, &["actor", "get", "user-1", "--by-custom-id"]);
    assert_success(&output, &["actor", "get", "--by-custom-id"]);
    assert_eq!(
        request_line(&request).split(' ').nth(1),
        Some("/api/v3/actors/user-1?by_custom_id=true")
    );
}

#[test]
fn update_patches_only_the_fields_passed() {
    let (request, output) = exchange(
        ONE_ACTOR,
        &[
            "actor",
            "update",
            "act-1",
            "--description",
            "updated",
            "--metadata",
            r#"{"tier":"enterprise"}"#,
        ],
    );
    assert_success(&output, &["actor", "update"]);

    let line = request_line(&request);
    assert!(line.starts_with("PATCH /api/v3/actors/act-1 "), "{line}");
    assert!(
        request.contains(r#"{"description":"updated","metadata":{"tier":"enterprise"}}"#),
        "{request}"
    );
    assert!(
        !request.contains("display_name"),
        "an omitted field must not be sent: {request}"
    );
}

#[test]
fn delete_uses_delete_and_prints_a_one_line_confirmation() {
    let (request, output) = exchange(NO_DATA, &["actor", "delete", "act-1"]);
    let stdout = assert_success(&output, &["actor", "delete"]);

    let line = request_line(&request);
    assert!(line.starts_with("DELETE /api/v3/actors/act-1 "), "{line}");
    assert_eq!(stdout.trim(), "Deleted actor `act-1`");
}

#[test]
fn bind_posts_actor_id_to_the_workspace_endpoint() {
    let (request, output) = exchange(
        r#"{"success":true,"data":{"actor_id":"act-1","bound_at":"2025-03-15T09:00:00Z"}}"#,
        &["actor", "bind", "--workspace", "ws-1", "--actor", "act-1"],
    );
    let stdout = assert_success(&output, &["actor", "bind"]);

    let line = request_line(&request);
    assert!(
        line.starts_with("POST /api/v3/workspaces/ws-1/actors "),
        "{line}"
    );
    assert!(request.contains(r#"{"actor_id":"act-1"}"#), "{request}");
    assert!(
        stdout.contains("\"bound_at\": \"2025-03-15T09:00:00Z\""),
        "{stdout}"
    );
}

#[test]
fn unbind_deletes_the_binding_and_prints_a_one_line_confirmation() {
    let (request, output) = exchange(
        NO_DATA,
        &["actor", "unbind", "--workspace", "ws-1", "--actor", "act-1"],
    );
    let stdout = assert_success(&output, &["actor", "unbind"]);

    let line = request_line(&request);
    assert!(
        line.starts_with("DELETE /api/v3/workspaces/ws-1/actors/act-1 "),
        "{line}"
    );
    assert_eq!(stdout.trim(), "Unbound actor `act-1` from workspace `ws-1`");
}

#[test]
fn ids_are_encoded_into_their_own_path_segment() {
    let (request, _) = exchange(ONE_ACTOR, &["actor", "get", "weird id/here?x"]);
    assert_eq!(
        request_line(&request).split(' ').nth(1),
        Some("/api/v3/actors/weird%20id%2Fhere%3Fx")
    );
}

#[test]
fn unknown_actor_type_from_the_server_is_printed_not_rejected() {
    // A type added server-side must not break the command, and the raw value
    // must survive into the output.
    let (_, output) = exchange(
        r#"{"success":true,"data":{"id":"act-1","actor_type":"SUPERVISOR","display_name":"Ada"}}"#,
        &["actor", "get", "act-1"],
    );
    let stdout = assert_success(&output, &["actor", "get"]);
    assert!(
        stdout.contains("\"actor_type\": \"SUPERVISOR\""),
        "unknown actor_type must round-trip into the output: {stdout}"
    );
}

#[test]
fn server_errors_are_surfaced_verbatim() {
    let server = StubServer::new(
        r#"{"success":false,"message":"custom_id already exists","error_code":"ACTOR_CUSTOM_ID_CONFLICT"}"#,
    );
    let home = logged_in_home(&server.base_url);
    let output = run(
        &home,
        &[
            "actor",
            "create",
            "--custom-id",
            "user-1",
            "--display-name",
            "Alice",
        ],
    );
    let _ = server.received();
    let _ = fs::remove_dir_all(&home);

    assert!(!output.status.success(), "duplicate create should fail");
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(stderr.contains("custom_id already exists"), "{stderr}");
    assert!(stderr.contains("[ACTOR_CUSTOM_ID_CONFLICT]"), "{stderr}");
}
