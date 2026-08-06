//! Test-only HTTP helpers.
//!
//! The crate deliberately has no HTTP mock dependency, so transport-level
//! invariants (which headers go out, which bytes go out) are asserted against a
//! hand-rolled one-shot loopback server.

use std::io::{Read, Write};
use std::net::TcpListener;
use std::thread::JoinHandle;

/// What the server saw.
pub(crate) struct CapturedRequest {
    /// Request line and headers, verbatim.
    pub head: String,
    /// Request body bytes.
    pub body: Vec<u8>,
}

impl CapturedRequest {
    /// Whether a header with this name was sent, case-insensitively.
    pub fn has_header(&self, name: &str) -> bool {
        let needle = format!("{}:", name.to_ascii_lowercase());
        self.head
            .lines()
            .any(|line| line.to_ascii_lowercase().starts_with(&needle))
    }
}

/// A `200 OK` JSON response with a correctly computed `Content-Length`.
///
/// Hand-counted lengths silently truncate the body and surface as a confusing
/// parse error, so fixtures should always go through this.
pub(crate) fn json_ok(body: &str) -> String {
    format!(
        "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\n\r\n{body}",
        body.len()
    )
}

/// Serve exactly one HTTP request from loopback, reply with `response`, and
/// hand back what the client sent.
///
/// Returns the base URL to point a client at, plus a handle that yields the
/// captured request once the exchange completes.
pub(crate) fn one_shot_server(
    response: impl Into<String>,
) -> (String, JoinHandle<CapturedRequest>) {
    let response = response.into();
    let listener = TcpListener::bind("127.0.0.1:0").expect("bind loopback");
    let addr = listener.local_addr().expect("local addr");

    let handle = std::thread::spawn(move || {
        let (mut stream, _) = listener.accept().expect("accept");

        let mut head = Vec::new();
        let mut byte = [0u8; 1];
        while !head.ends_with(b"\r\n\r\n") {
            if stream.read(&mut byte).expect("read head") == 0 {
                break;
            }
            head.push(byte[0]);
        }
        let head = String::from_utf8_lossy(&head).into_owned();

        let len = head
            .lines()
            .find(|line| line.to_ascii_lowercase().starts_with("content-length:"))
            .and_then(|line| line.split(':').nth(1))
            .and_then(|value| value.trim().parse::<usize>().ok())
            .unwrap_or(0);
        let mut body = vec![0u8; len];
        if len > 0 {
            stream.read_exact(&mut body).expect("read body");
        }

        stream
            .write_all(response.as_bytes())
            .expect("write response");
        stream.flush().expect("flush");
        CapturedRequest { head, body }
    });

    (format!("http://{addr}"), handle)
}
