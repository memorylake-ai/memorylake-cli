//! Wire-level `project document` tests against a loopback stub of the API.
//!
//! These need no API key. They pin the HTTP method, path, and body each
//! subcommand sends, the output it prints, and — for the guards that are
//! supposed to stop before importing — the requests it does **not** send.
//!
//! `import` makes several calls in sequence (classify each id, then post the
//! batch), so the stub here serves a scripted list of responses rather than the
//! single one [`crate::actor::wire`] needs.

use std::fs;
use std::io::{ErrorKind, Read, Write};
use std::net::{TcpListener, TcpStream};
use std::path::{Path, PathBuf};
use std::process::{Command, Output};
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::{Receiver, channel};
use std::thread::JoinHandle;
use std::time::Duration;

use crate::common::{run, temp_home};

/// Loopback HTTP server that answers a scripted sequence of responses.
///
/// Scripting more responses than the CLI ends up requesting is normal here —
/// several tests assert that a request was never made — so the accept loop
/// polls a shutdown flag instead of blocking forever on a connection that will
/// not arrive.
struct ScriptedServer {
    base_url: String,
    requests: Receiver<String>,
    shutdown: Arc<AtomicBool>,
    handle: Option<JoinHandle<()>>,
}

impl ScriptedServer {
    /// Serve `bodies`, each wrapped in a `200 OK` JSON response.
    fn new(bodies: &[&str]) -> Self {
        let responses: Vec<String> = bodies.iter().map(|body| http_200(body)).collect();
        Self::raw(&responses.iter().map(String::as_str).collect::<Vec<_>>())
    }

    /// Serve complete HTTP responses verbatim.
    ///
    /// Needed by the download tests, which depend on the status line and on
    /// headers — `Content-Disposition` above all — that the JSON wrapper in
    /// [`Self::new`] would replace.
    fn raw(responses: &[&str]) -> Self {
        let listener = TcpListener::bind("127.0.0.1:0").expect("bind stub server");
        let addr = listener.local_addr().expect("stub server address");
        let responses: Vec<String> = responses.iter().map(|r| (*r).to_string()).collect();

        let (sender, requests) = channel();
        let shutdown = Arc::new(AtomicBool::new(false));
        let stop = Arc::clone(&shutdown);
        let handle = std::thread::spawn(move || {
            listener
                .set_nonblocking(true)
                .expect("non-blocking listener");
            let mut remaining = responses.into_iter();

            while !stop.load(Ordering::Relaxed) {
                match listener.accept() {
                    Ok((mut stream, _)) => {
                        stream.set_nonblocking(false).expect("blocking stream");
                        let Some(response) = remaining.next() else {
                            return;
                        };
                        let request = read_http_request(&mut stream);
                        // Best effort: a dropped receiver means the test ended.
                        let _ = sender.send(request);
                        let _ = stream.write_all(response.as_bytes());
                        let _ = stream.flush();
                    }
                    Err(err) if err.kind() == ErrorKind::WouldBlock => {
                        std::thread::sleep(Duration::from_millis(5));
                    }
                    Err(_) => return,
                }
            }
        });

        Self {
            base_url: format!("http://{addr}"),
            requests,
            shutdown,
            handle: Some(handle),
        }
    }

    /// Every request the CLI sent, in order.
    ///
    /// Call only after the CLI process has exited; everything it was going to
    /// send has been sent by then.
    fn collected(&self) -> Vec<String> {
        let mut all = Vec::new();
        while let Ok(request) = self.requests.recv_timeout(Duration::from_millis(200)) {
            all.push(request);
        }
        all
    }
}

impl Drop for ScriptedServer {
    fn drop(&mut self) {
        self.shutdown.store(true, Ordering::Relaxed);
        if let Some(handle) = self.handle.take() {
            // Best effort: a panicking server thread already failed the test.
            let _ = handle.join();
        }
    }
}

fn http_200(body: &str) -> String {
    format!(
        "HTTP/1.1 200 OK\r\ncontent-type: application/json\r\ncontent-length: {}\r\nconnection: close\r\n\r\n{body}",
        body.len()
    )
}

/// A binary response, optionally naming the file the way the API does.
fn http_binary(body: &str, disposition: Option<&str>) -> String {
    let disposition = disposition
        .map(|value| format!("content-disposition: {value}\r\n"))
        .unwrap_or_default();
    format!(
        "HTTP/1.1 200 OK\r\ncontent-type: application/octet-stream\r\n{disposition}content-length: {}\r\nconnection: close\r\n\r\n{body}",
        body.len()
    )
}

/// Like [`exchange`], but runs the command inside `directory`.
///
/// `download` writes into the current directory when told nothing else, so a
/// test of that path has to own the directory it runs in — otherwise it drops
/// files into the repository.
fn exchange_in(directory: &Path, responses: &[&str], args: &[&str]) -> (Vec<String>, Output) {
    let server = ScriptedServer::raw(responses);
    let home = logged_in_home(&server.base_url);
    let output = Command::new(env!("CARGO_BIN_EXE_memorylake"))
        .current_dir(directory)
        .env("MEMORYLAKE_CONFIG_DIR", home.join(".memorylake"))
        .env("HOME", &home)
        .env("USERPROFILE", &home)
        .env_remove("MEMORYLAKE_API_KEY")
        .env_remove("MEMORYLAKE_BASE_URL")
        .env_remove("MEMORYLAKE_WORKSPACE")
        .args(args)
        .output()
        .expect("spawn memorylake");
    let requests = server.collected();
    let _ = fs::remove_dir_all(&home);
    (requests, output)
}

/// A scratch directory the caller owns for the duration of a test.
fn scratch_dir(tag: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!(
        "memorylake-dl-{tag}-{}-{:?}",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .expect("clock")
            .as_nanos()
    ));
    fs::create_dir_all(&dir).expect("create scratch dir");
    dir
}

fn http_404(body: &str) -> String {
    format!(
        "HTTP/1.1 404 Not Found\r\ncontent-type: application/json\r\ncontent-length: {}\r\nconnection: close\r\n\r\n{body}",
        body.len()
    )
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

/// Run one command against a scripted stub; report the requests and the output.
fn exchange(responses: &[&str], args: &[&str]) -> (Vec<String>, Output) {
    let server = ScriptedServer::new(responses);
    let home = logged_in_home(&server.base_url);
    let output = run(&home, args);
    let requests = server.collected();
    let _ = fs::remove_dir_all(&home);
    (requests, output)
}

fn request_line(request: &str) -> &str {
    request.lines().next().unwrap_or_default()
}

fn stdout_of(output: &Output) -> String {
    String::from_utf8_lossy(&output.stdout).into_owned()
}

fn stderr_of(output: &Output) -> String {
    String::from_utf8_lossy(&output.stderr).into_owned()
}

/// True when the CLI posted a batch to the import endpoint.
fn posted_an_import(requests: &[String]) -> bool {
    requests
        .iter()
        .any(|request| request_line(request).starts_with("POST /api/v3/workspaces/"))
}

const DOCUMENTS_PATH: &str = "/api/v3/workspaces/ws-1/projects/proj-1/memories/documents";

// `GET /drives/items/{id}` nests the resource one level deeper than the
// workspace endpoints do: `data.item`, not `data`.
const FILE_ITEM: &str = r#"{"success":true,"data":{"item":{"uri":"drive://d/sc-a:inode-1","item_id":"sc-a:inode-1","name":"a.txt","type":"file"}}}"#;
const FOLDER_ITEM: &str = r#"{"success":true,"data":{"item":{"uri":"drive://d/sc-a:inode-d","item_id":"sc-a:inode-d","name":"reports","type":"directory"}}}"#;

const IMPORT_ALL_OK: &str = r#"{"success":true,"data":{"success_count":1,"failure_count":0,"duplicate_count":0,"details":[{"result":"success","drive_item_id":"sc-a:inode-1","document_id":"doc-1"}],"details_truncated":false}}"#;
const IMPORT_PARTIAL_FAILURE: &str = r#"{"success":true,"data":{"success_count":1,"failure_count":1,"duplicate_count":0,"details":[{"result":"success","drive_item_id":"sc-a:inode-1","document_id":"doc-1"},{"result":"failed","drive_item_id":"sc-a:inode-2"}],"details_truncated":false}}"#;
const IMPORT_DUPLICATES_ONLY: &str = r#"{"success":true,"data":{"success_count":0,"failure_count":0,"duplicate_count":1,"details":[{"result":"duplicate","drive_item_id":"sc-a:inode-1","document_id":"doc-1"}],"details_truncated":false}}"#;

#[test]
fn import_posts_the_documented_body_to_the_documented_path() {
    let (requests, output) = exchange(
        &[FILE_ITEM, IMPORT_ALL_OK],
        &[
            "project",
            "document",
            "import",
            "--workspace",
            "ws-1",
            "--project",
            "proj-1",
            "sc-a:inode-1",
        ],
    );
    assert!(output.status.success(), "{}", stderr_of(&output));

    let import = requests
        .iter()
        .find(|request| request_line(request).starts_with("POST "))
        .expect("an import request");
    assert!(
        request_line(import).starts_with(&format!("POST {DOCUMENTS_PATH} ")),
        "{}",
        request_line(import)
    );
    assert!(
        import.contains(r#"{"drive_item_ids":["sc-a:inode-1"]}"#),
        "{import}"
    );
    // The colon in a Library item id must reach the body untouched.
    assert!(
        stdout_of(&output).contains("\"success_count\": 1"),
        "{}",
        stdout_of(&output)
    );
}

#[test]
fn a_partial_failure_prints_the_payload_and_still_exits_non_zero() {
    // The whole point of the exit-code rule: the batch already ran, so the
    // caller must get both the result and the failure signal.
    let (_, output) = exchange(
        &[FILE_ITEM, IMPORT_PARTIAL_FAILURE],
        &[
            "project",
            "document",
            "import",
            "--workspace",
            "ws-1",
            "--project",
            "proj-1",
            "sc-a:inode-1",
        ],
    );

    assert!(
        !output.status.success(),
        "failure_count > 0 must fail the command"
    );
    let stdout = stdout_of(&output);
    assert!(
        stdout.contains("\"failure_count\": 1"),
        "the API payload must still reach stdout: {stdout}"
    );
    assert!(
        stdout.contains("\"document_id\": \"doc-1\""),
        "the successful entries must still be visible: {stdout}"
    );
    assert!(
        stderr_of(&output).contains("could not be imported"),
        "{}",
        stderr_of(&output)
    );
}

#[test]
fn duplicates_alone_exit_zero() {
    let (_, output) = exchange(
        &[FILE_ITEM, IMPORT_DUPLICATES_ONLY],
        &[
            "project",
            "document",
            "import",
            "--workspace",
            "ws-1",
            "--project",
            "proj-1",
            "sc-a:inode-1",
        ],
    );
    assert!(
        output.status.success(),
        "re-importing an existing file is not a failure: {}",
        stderr_of(&output)
    );
    assert!(stdout_of(&output).contains("\"duplicate_count\": 1"));
}

#[test]
fn a_folder_without_recursive_never_reaches_the_import_endpoint() {
    // Scripts an import response that must go unused.
    let (requests, output) = exchange(
        &[FOLDER_ITEM, IMPORT_ALL_OK],
        &[
            "project",
            "document",
            "import",
            "--workspace",
            "ws-1",
            "--project",
            "proj-1",
            "sc-a:inode-d",
        ],
    );

    assert!(!output.status.success(), "a folder id must be refused");
    assert!(
        stderr_of(&output).contains("--recursive"),
        "{}",
        stderr_of(&output)
    );
    assert!(
        !posted_an_import(&requests),
        "nothing may be imported when a folder is refused, but the CLI sent: {requests:?}"
    );
}

#[test]
fn exceeding_max_files_never_reaches_the_import_endpoint() {
    let children = r#"{"success":true,"data":{"items":[
        {"uri":"drive://d/1","item_id":"sc-a:inode-1","name":"a","type":"file"},
        {"uri":"drive://d/2","item_id":"sc-a:inode-2","name":"b","type":"file"},
        {"uri":"drive://d/3","item_id":"sc-a:inode-3","name":"c","type":"file"}
    ],"continuation_token":null}}"#;

    let (requests, output) = exchange(
        &[FOLDER_ITEM, children, IMPORT_ALL_OK],
        &[
            "project",
            "document",
            "import",
            "--workspace",
            "ws-1",
            "--project",
            "proj-1",
            "sc-a:inode-d",
            "--recursive",
            "--max-files",
            "2",
        ],
    );

    assert!(!output.status.success(), "3 files against a cap of 2");
    let stderr = stderr_of(&output);
    assert!(stderr.contains("--max-files"), "{stderr}");
    assert!(
        !posted_an_import(&requests),
        "the cap must be enforced before importing, but the CLI sent: {requests:?}"
    );
}

#[test]
fn recursive_expansion_posts_every_file_the_folder_pages_reveal() {
    // Two pages: stopping after the first would silently import a subset.
    let page_one = r#"{"success":true,"data":{"items":[
        {"uri":"drive://d/1","item_id":"sc-a:inode-1","name":"a","type":"file"}
    ],"continuation_token":"tok-2"}}"#;
    let page_two = r#"{"success":true,"data":{"items":[
        {"uri":"drive://d/2","item_id":"sc-a:inode-2","name":"b","type":"file"}
    ],"continuation_token":null}}"#;

    let (requests, output) = exchange(
        &[FOLDER_ITEM, page_one, page_two, IMPORT_ALL_OK],
        &[
            "project",
            "document",
            "import",
            "--workspace",
            "ws-1",
            "--project",
            "proj-1",
            "sc-a:inode-d",
            "--recursive",
        ],
    );
    assert!(output.status.success(), "{}", stderr_of(&output));

    let second_page = requests
        .iter()
        .find(|request| request_line(request).contains("continuation_token=tok-2"))
        .expect("the second page must be fetched");
    assert!(
        request_line(second_page).starts_with("GET /api/v1/drives/items/sc-a:inode-d/children?"),
        "{}",
        request_line(second_page)
    );

    let import = requests
        .iter()
        .find(|request| request_line(request).starts_with("POST "))
        .expect("an import request");
    assert!(
        import.contains(r#"{"drive_item_ids":["sc-a:inode-1","sc-a:inode-2"]}"#),
        "both pages' files must be imported: {import}"
    );
}

#[test]
fn list_sends_the_documented_query_parameters() {
    let page = r#"{"success":true,"data":{"items":[],"continuation_token":null}}"#;
    let (requests, output) = exchange(
        &[page],
        &[
            "proj",
            "doc",
            "list",
            "--workspace",
            "ws-1",
            "--project",
            "proj-1",
            "--page-size",
            "25",
            "--continuation-token",
            "tok-1",
            "--name",
            "report",
        ],
    );
    assert!(output.status.success(), "{}", stderr_of(&output));

    let line = request_line(&requests[0]);
    assert!(
        line.starts_with(&format!("GET {DOCUMENTS_PATH}?")),
        "{line}"
    );
    for expected in [
        "page_size=25",
        "continuation_token=tok-1",
        "name_fuzzy=report",
    ] {
        assert!(line.contains(expected), "missing {expected} in {line}");
    }
}

#[test]
fn get_addresses_the_document_by_id() {
    let document = r#"{"success":true,"data":{"id":"doc-1","name":"a.txt","status":"running"}}"#;
    let (requests, output) = exchange(
        &[document],
        &[
            "project",
            "document",
            "get",
            "--workspace",
            "ws-1",
            "--project",
            "proj-1",
            "doc-1",
        ],
    );
    assert!(output.status.success(), "{}", stderr_of(&output));
    assert_eq!(
        request_line(&requests[0]).split(' ').nth(1),
        Some(format!("{DOCUMENTS_PATH}/doc-1").as_str())
    );
    assert!(stdout_of(&output).contains("\"status\": \"running\""));
}

#[test]
fn delete_sends_the_ids_in_the_request_body() {
    let (requests, output) = exchange(
        &[r#"{"success":true,"data":{}}"#],
        &[
            "project",
            "document",
            "delete",
            "--workspace",
            "ws-1",
            "--project",
            "proj-1",
            "doc-1",
            "doc-2",
        ],
    );
    assert!(output.status.success(), "{}", stderr_of(&output));

    let line = request_line(&requests[0]);
    assert!(
        line.starts_with(&format!("DELETE {DOCUMENTS_PATH} ")),
        "delete addresses the collection, not a single document: {line}"
    );
    assert!(
        requests[0].contains(r#"{"ids":["doc-1","doc-2"]}"#),
        "the ids must travel in the body: {}",
        requests[0]
    );
    assert!(
        stdout_of(&output).contains("Deleted 2 document(s)"),
        "{}",
        stdout_of(&output)
    );
}

#[test]
fn a_status_the_cli_does_not_know_is_printed_rather_than_rejected() {
    let document = r#"{"success":true,"data":{"id":"doc-1","name":"a.txt","status":"reindexing"}}"#;
    let (_, output) = exchange(
        &[document],
        &[
            "project",
            "document",
            "get",
            "--workspace",
            "ws-1",
            "--project",
            "proj-1",
            "doc-1",
        ],
    );
    assert!(output.status.success(), "{}", stderr_of(&output));
    assert!(
        stdout_of(&output).contains("\"status\": \"reindexing\""),
        "an unfamiliar status must round-trip into the output: {}",
        stdout_of(&output)
    );
}

#[test]
fn server_errors_are_surfaced_verbatim() {
    let (_, output) = exchange(
        &[
            FILE_ITEM,
            r#"{"success":false,"message":"project not found","error_code":"PROJECT_NOT_FOUND"}"#,
        ],
        &[
            "project",
            "document",
            "import",
            "--workspace",
            "ws-1",
            "--project",
            "proj-1",
            "sc-a:inode-1",
        ],
    );

    assert!(!output.status.success(), "a rejected import must fail");
    let stderr = stderr_of(&output);
    assert!(stderr.contains("project not found"), "{stderr}");
    assert!(stderr.contains("[PROJECT_NOT_FOUND]"), "{stderr}");
}

const DOWNLOAD_PATH: &str =
    "/api/v3/workspaces/ws-1/projects/proj-1/memories/documents/doc-1/download";

#[test]
fn download_requests_the_documented_path() {
    let dir = scratch_dir("path");
    let (requests, output) = exchange_in(
        &dir,
        &[&http_binary(
            "file body",
            Some(r#"attachment; filename="notes.txt""#),
        )],
        &[
            "project",
            "document",
            "download",
            "--workspace",
            "ws-1",
            "--project",
            "proj-1",
            "doc-1",
        ],
    );
    assert!(output.status.success(), "{}", stderr_of(&output));

    assert_eq!(
        request_line(&requests[0]),
        format!("GET {DOWNLOAD_PATH} HTTP/1.1"),
        "unexpected request line"
    );
    let _ = fs::remove_dir_all(&dir);
}

#[test]
fn download_names_the_file_from_content_disposition() {
    // Without this the caller would have to know the name in advance, which is
    // the whole point of the server sending one.
    let dir = scratch_dir("named");
    let (_, output) = exchange_in(
        &dir,
        &[&http_binary(
            "file body",
            Some(r#"attachment; filename="notes.txt""#),
        )],
        &[
            "project",
            "document",
            "download",
            "--workspace",
            "ws-1",
            "--project",
            "proj-1",
            "doc-1",
        ],
    );
    assert!(output.status.success(), "{}", stderr_of(&output));

    assert_eq!(
        fs::read_to_string(dir.join("notes.txt")).expect("file written under the server's name"),
        "file body"
    );
    let _ = fs::remove_dir_all(&dir);
}

#[test]
fn download_writes_bytes_to_stdout_when_asked() {
    // Piping is the reason `-o -` exists, so stdout must carry the file and
    // nothing else — the summary belongs on stderr.
    let dir = scratch_dir("stdout");
    let (_, output) = exchange_in(
        &dir,
        &[&http_binary(
            "piped body",
            Some(r#"attachment; filename="notes.txt""#),
        )],
        &[
            "project",
            "document",
            "download",
            "--workspace",
            "ws-1",
            "--project",
            "proj-1",
            "doc-1",
            "-o",
            "-",
        ],
    );
    assert!(output.status.success(), "{}", stderr_of(&output));

    assert_eq!(stdout_of(&output), "piped body");
    assert!(
        stderr_of(&output).contains("bytes to stdout"),
        "the summary belongs on stderr: {}",
        stderr_of(&output)
    );
    assert!(
        !dir.join("notes.txt").exists(),
        "streaming to stdout must not also write a file"
    );
    let _ = fs::remove_dir_all(&dir);
}

#[test]
fn download_refuses_to_overwrite_without_force() {
    let dir = scratch_dir("clobber");
    fs::write(dir.join("notes.txt"), "do not lose me").expect("seed the destination");

    let (_, output) = exchange_in(
        &dir,
        &[&http_binary(
            "new body",
            Some(r#"attachment; filename="notes.txt""#),
        )],
        &[
            "project",
            "document",
            "download",
            "--workspace",
            "ws-1",
            "--project",
            "proj-1",
            "doc-1",
        ],
    );
    assert!(!output.status.success(), "overwriting must fail");
    assert!(
        stderr_of(&output).contains("--force"),
        "the error names the way out: {}",
        stderr_of(&output)
    );
    assert_eq!(
        fs::read_to_string(dir.join("notes.txt")).expect("read"),
        "do not lose me",
        "the existing file must survive"
    );
    let _ = fs::remove_dir_all(&dir);
}

#[test]
fn download_overwrites_with_force() {
    let dir = scratch_dir("force");
    fs::write(dir.join("notes.txt"), "stale").expect("seed the destination");

    let (_, output) = exchange_in(
        &dir,
        &[&http_binary(
            "fresh body",
            Some(r#"attachment; filename="notes.txt""#),
        )],
        &[
            "project",
            "document",
            "download",
            "--workspace",
            "ws-1",
            "--project",
            "proj-1",
            "doc-1",
            "--force",
        ],
    );
    assert!(output.status.success(), "{}", stderr_of(&output));
    assert_eq!(
        fs::read_to_string(dir.join("notes.txt")).expect("read"),
        "fresh body"
    );
    let _ = fs::remove_dir_all(&dir);
}

#[test]
fn download_without_a_server_name_asks_for_output() {
    // Guessing a name would put a file somewhere the caller did not choose.
    let dir = scratch_dir("unnamed");
    let (_, output) = exchange_in(
        &dir,
        &[&http_binary("body", None)],
        &[
            "project",
            "document",
            "download",
            "--workspace",
            "ws-1",
            "--project",
            "proj-1",
            "doc-1",
        ],
    );
    assert!(
        !output.status.success(),
        "an unnamed file must not be guessed"
    );
    assert!(
        stderr_of(&output).contains("--output"),
        "the error says how to proceed: {}",
        stderr_of(&output)
    );

    let leftovers: Vec<_> = fs::read_dir(&dir)
        .expect("read scratch dir")
        .filter_map(|entry| entry.ok())
        .map(|entry| entry.file_name())
        .collect();
    assert!(
        leftovers.is_empty(),
        "a failed download must leave nothing behind: {leftovers:?}"
    );
    let _ = fs::remove_dir_all(&dir);
}

#[test]
fn download_leaves_no_partial_file_when_the_server_errors() {
    let dir = scratch_dir("error");
    let (_, output) = exchange_in(
        &dir,
        &[&http_404(
            r#"{"success":false,"message":"Document not found","error_code":"NOT_FOUND"}"#,
        )],
        &[
            "project",
            "document",
            "download",
            "--workspace",
            "ws-1",
            "--project",
            "proj-1",
            "doc-1",
        ],
    );
    assert!(!output.status.success(), "a 404 must fail the command");
    assert!(
        stderr_of(&output).contains("Document not found"),
        "the server's message reaches the caller: {}",
        stderr_of(&output)
    );

    let leftovers: Vec<_> = fs::read_dir(&dir)
        .expect("read scratch dir")
        .filter_map(|entry| entry.ok())
        .map(|entry| entry.file_name())
        .collect();
    assert!(
        leftovers.is_empty(),
        "the temporary file must be cleaned up: {leftovers:?}"
    );
    let _ = fs::remove_dir_all(&dir);
}
