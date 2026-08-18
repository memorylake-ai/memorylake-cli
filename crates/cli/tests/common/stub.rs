//! Loopback HTTP stub for wire-level CLI tests.
//!
//! These need no API key: they pin the HTTP method, path, query string, and
//! body that a subcommand sends, and the output it prints back. Live tests
//! prove the endpoints exist; wire tests prove the CLI calls the documented
//! ones with the documented payload.

use std::fs;
use std::io::{Read, Write};
use std::net::{TcpListener, TcpStream};
use std::path::PathBuf;
use std::process::Output;
use std::sync::mpsc::{Receiver, RecvTimeoutError, channel};
use std::thread::JoinHandle;
use std::time::Duration;

use super::{run, temp_home};

/// One-shot loopback HTTP server that answers with a canned envelope and hands
/// the raw request text back to the test.
struct StubServer {
    base_url: String,
    requests: Receiver<String>,
    handle: Option<JoinHandle<()>>,
}

impl StubServer {
    fn new(body: &str) -> Self {
        Self::with_responses(&[body])
    }

    /// Answer `bodies.len()` requests, one canned body each, in order.
    ///
    /// Commands that poll send several requests in one run; each gets the next
    /// body, so a test can script a sequence like "not finished, then
    /// finished". Every response closes its connection, so each request
    /// arrives on a fresh one.
    fn with_responses(bodies: &[&str]) -> Self {
        let listener = TcpListener::bind("127.0.0.1:0").expect("bind stub server");
        let addr = listener.local_addr().expect("stub server address");
        let responses: Vec<String> = bodies
            .iter()
            .map(|body| {
                format!(
                    "HTTP/1.1 200 OK\r\ncontent-type: application/json\r\ncontent-length: {}\r\nconnection: close\r\n\r\n{body}",
                    body.len()
                )
            })
            .collect();

        let (sender, requests) = channel();
        let handle = std::thread::spawn(move || {
            for response in responses {
                let Ok((mut stream, _)) = listener.accept() else {
                    return;
                };
                let request = read_http_request(&mut stream);
                // Best effort: a dropped receiver means the test already ended.
                let _ = sender.send(request);
                let _ = stream.write_all(response.as_bytes());
                let _ = stream.flush();
            }
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

    /// Collect exactly `count` requests, failing if fewer arrive in time.
    ///
    /// The timeout is generous because a polling command sleeps between
    /// requests.
    fn received_all(&self, count: usize) -> Vec<String> {
        (0..count)
            .map(
                |index| match self.requests.recv_timeout(Duration::from_secs(30)) {
                    Ok(request) => request,
                    Err(RecvTimeoutError::Timeout) => {
                        panic!("stub server received {index} of {count} expected requests")
                    }
                    Err(RecvTimeoutError::Disconnected) => panic!("stub server thread died"),
                },
            )
            .collect()
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
///
/// Also useful without a stub: pointing it at an unreachable URL proves that a
/// command which fails locally never reached the network.
pub fn logged_in_home(base_url: &str) -> PathBuf {
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

/// A temp `$HOME` logged in and already remembering `workspace`, as though
/// `workspace use` had run.
pub fn logged_in_home_with_workspace(base_url: &str, workspace: &str) -> PathBuf {
    let home = temp_home();
    let root = home.join(".memorylake");
    fs::create_dir_all(&root).expect("create .memorylake");
    fs::write(
        root.join("config.toml"),
        format!(
            "active_profile = \"default\"\n\n[profiles.default]\nbase_url = \"{base_url}\"\nworkspace = \"{workspace}\"\n"
        ),
    )
    .expect("write config.toml");
    fs::write(
        root.join("credentials.toml"),
        "[profiles.default]\napi_key = \"sk_offline_stub_key\"\nlogin_method = \"api_key\"\n",
    )
    .expect("write credentials.toml");
    home
}

/// Run one command against a stub, from a `$HOME` that already remembers a
/// workspace. Reports the request the CLI sent and the process output.
pub fn exchange_with_remembered_workspace(
    response: &str,
    workspace: &str,
    args: &[&str],
) -> (String, Output) {
    let server = StubServer::new(response);
    let home = logged_in_home_with_workspace(&server.base_url, workspace);
    let output = run(&home, args);
    let request = server.received();
    let _ = fs::remove_dir_all(&home);
    (request, output)
}

/// Run one command against a stub returning `response`, and report both the
/// request the CLI sent and the process output.
pub fn exchange(response: &str, args: &[&str]) -> (String, Output) {
    let server = StubServer::new(response);
    let home = logged_in_home(&server.base_url);
    let output = run(&home, args);
    let request = server.received();
    let _ = fs::remove_dir_all(&home);
    (request, output)
}

/// Run one command against a stub that answers `responses` in order, and
/// report every request the CLI sent alongside the process output.
///
/// For commands that poll: the request count is pinned by the length of
/// `responses`, so an extra or missing round trip fails the test.
pub fn exchange_sequence(responses: &[&str], args: &[&str]) -> (Vec<String>, Output) {
    let server = StubServer::with_responses(responses);
    let home = logged_in_home(&server.base_url);
    let output = run(&home, args);
    let requests = server.received_all(responses.len());
    let _ = fs::remove_dir_all(&home);
    (requests, output)
}

/// First line of a raw HTTP request (`METHOD path HTTP/1.1`).
pub fn request_line(request: &str) -> &str {
    request.lines().next().unwrap_or_default()
}

/// Body of a raw HTTP request: everything after the blank line.
pub fn request_body(request: &str) -> &str {
    request
        .split_once("\r\n\r\n")
        .map(|(_, body)| body)
        .unwrap_or_default()
}
