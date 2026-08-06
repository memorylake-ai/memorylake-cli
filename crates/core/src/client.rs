//! Blocking HTTP client for the MemoryLake OpenAPI.

use std::borrow::Cow;
use std::collections::BTreeMap;

use reqwest::StatusCode;
use reqwest::blocking::{Body, Client as HttpClient};
use reqwest::header::{AUTHORIZATION, CONTENT_TYPE, ETAG, HeaderMap, HeaderValue};
use serde::Serialize;
use serde::de::DeserializeOwned;
use serde_json::Value;

use crate::error::{Error, Result};

/// Thin authenticated HTTP client for MemoryLake v3 APIs.
#[derive(Debug, Clone)]
pub struct Client {
    http: HttpClient,
    base_url: String,
    api_key: String,
}

impl Client {
    /// Create a client for `base_url` authenticated with `api_key`.
    pub fn new(base_url: impl Into<String>, api_key: impl Into<String>) -> Result<Self> {
        let mut headers = HeaderMap::new();
        headers.insert(CONTENT_TYPE, HeaderValue::from_static("application/json"));

        let http = HttpClient::builder().default_headers(headers).build()?;

        Ok(Self {
            http,
            base_url: base_url.into().trim_end_matches('/').to_string(),
            api_key: api_key.into(),
        })
    }

    /// Base URL this client targets.
    pub fn base_url(&self) -> &str {
        &self.base_url
    }

    /// Perform a GET and deserialize the API `data` payload.
    pub fn get_data<T>(&self, path: &str, query: &[(&str, String)]) -> Result<T>
    where
        T: DeserializeOwned,
    {
        let url = self.url(path);
        let mut builder = self.http.get(&url).headers(self.auth_headers()?);
        for (key, value) in query {
            builder = builder.query(&[(key, value)]);
        }
        self.send(builder.build()?)
    }

    /// Perform a POST with a JSON body and deserialize the API `data` payload.
    pub fn post_data<T, B>(&self, path: &str, body: &B) -> Result<T>
    where
        T: DeserializeOwned,
        B: Serialize,
    {
        let url = self.url(path);
        let request = self
            .http
            .post(&url)
            .headers(self.auth_headers()?)
            .json(body)
            .build()?;
        self.send(request)
    }

    /// Perform a PATCH with a JSON body and deserialize the API `data` payload.
    pub fn patch_data<T, B>(&self, path: &str, body: &B) -> Result<T>
    where
        T: DeserializeOwned,
        B: Serialize,
    {
        let url = self.url(path);
        let request = self
            .http
            .patch(&url)
            .headers(self.auth_headers()?)
            .json(body)
            .build()?;
        self.send(request)
    }

    /// Perform a DELETE and deserialize the API `data` payload.
    ///
    /// Endpoints that answer `{"success": true, "message": ...}` with no `data`
    /// decode into `()`.
    pub fn delete_data<T>(&self, path: &str) -> Result<T>
    where
        T: DeserializeOwned,
    {
        let url = self.url(path);
        let request = self
            .http
            .delete(&url)
            .headers(self.auth_headers()?)
            .build()?;
        self.send(request)
    }

    /// Trace, execute, and decode a prepared request.
    ///
    /// Every verb funnels through here so the `Authorization` header is
    /// redacted in exactly one place and no method can bypass it.
    fn send<T>(&self, request: reqwest::blocking::Request) -> Result<T>
    where
        T: DeserializeOwned,
    {
        tracing::trace!(
            method = %request.method(),
            url = %request.url(),
            headers = ?redact_headers(request.headers()),
            body = %request_body_as_str(&request),
            "HTTP request"
        );
        let response = self.http.execute(request)?;
        decode_envelope(response)
    }

    /// Upload one part of a chunked upload to its pre-signed `upload_url` and
    /// return the `ETag` the storage backend assigned to it.
    ///
    /// No `Authorization` header is sent: `upload_url` carries its own
    /// signature over a fixed header set, and MemoryLake credentials have no
    /// meaning at the storage backend. The response is raw storage-backend
    /// output, not a MemoryLake envelope, so it does not go through
    /// [`decode_envelope`].
    ///
    /// `body` is consumed by the attempt; callers that retry must supply a
    /// fresh one.
    pub fn put_presigned_part(
        &self,
        upload_url: &str,
        body: Body,
    ) -> std::result::Result<String, PartUploadError> {
        // `self.http`'s default headers carry only `Content-Type`; auth is
        // applied per-request via `auth_headers`, which is deliberately not
        // called here. Override the JSON default so the part is not mislabeled.
        let response = self
            .http
            .put(upload_url)
            .header(CONTENT_TYPE, "application/octet-stream")
            .body(body)
            .send()?;

        let status = response.status();
        tracing::trace!(
            status = status.as_u16(),
            url = %redact_presigned(upload_url),
            "part upload response"
        );

        if !status.is_success() {
            let body = response.text().unwrap_or_default();
            return Err(PartUploadError::Status {
                status,
                body: redact_presigned(&body).into_owned(),
            });
        }

        response
            .headers()
            .get(ETAG)
            .and_then(|value| value.to_str().ok())
            .map(str::to_string)
            .ok_or(PartUploadError::MissingETag)
    }

    fn url(&self, path: &str) -> String {
        let path = if path.starts_with('/') {
            path.to_string()
        } else {
            format!("/{path}")
        };
        format!("{}{path}", self.base_url)
    }

    fn auth_headers(&self) -> Result<HeaderMap> {
        let mut headers = HeaderMap::new();
        let value = HeaderValue::from_str(&format!("Bearer {}", self.api_key)).map_err(|err| {
            Error::Api {
                message: format!("invalid API key header value: {err}"),
            }
        })?;
        headers.insert(AUTHORIZATION, value);
        Ok(headers)
    }
}

/// Why a single pre-signed part upload attempt failed.
///
/// Kept separate from [`Error`] so the upload orchestrator can decide whether
/// another attempt is worth making before flattening this into a user-facing
/// message.
#[derive(Debug, thiserror::Error)]
pub enum PartUploadError {
    /// The request never produced a response (connection, timeout, broken pipe).
    #[error("{0}")]
    Transport(#[from] reqwest::Error),

    /// The storage backend answered with a non-success status.
    #[error("storage backend returned HTTP {status}")]
    Status {
        /// Status returned by the storage backend.
        status: StatusCode,
        /// Response body, with credential-bearing parameters redacted.
        body: String,
    },

    /// The upload succeeded but no `ETag` came back, so the part cannot be
    /// referenced when finalizing.
    #[error("storage backend accepted the part but returned no ETag header")]
    MissingETag,
}

impl PartUploadError {
    /// Whether re-sending the same part to the same pre-signed URL could
    /// plausibly succeed.
    ///
    /// Expired or otherwise rejected URLs (4xx) are *not* retryable: the
    /// signature is fixed, so every attempt fails identically. Only a fresh
    /// upload session can recover, which is the caller's decision to make.
    pub fn is_retryable(&self) -> bool {
        match self {
            Self::Transport(_) => true,
            Self::Status { status, .. } => {
                status.is_server_error() || *status == StatusCode::TOO_MANY_REQUESTS
            }
            Self::MissingETag => false,
        }
    }

    /// Status returned by the storage backend, when there was a response.
    pub fn status(&self) -> Option<StatusCode> {
        match self {
            Self::Status { status, .. } => Some(*status),
            _ => None,
        }
    }
}

/// Successful MemoryLake OpenAPI envelope: `{"success":true,"data":...}`.
#[derive(Debug, serde::Deserialize)]
struct ApiEnvelope {
    success: bool,
    #[serde(default)]
    message: Option<String>,
    #[serde(default)]
    data: Option<Value>,
    #[serde(default)]
    error_code: Option<String>,
}

/// Auth gateway error shape observed on 401:
/// `{"error":{"message":"...","type":"authentication_error"}}`.
#[derive(Debug, serde::Deserialize)]
struct AuthErrorEnvelope {
    error: AuthErrorBody,
}

#[derive(Debug, serde::Deserialize)]
struct AuthErrorBody {
    message: Option<String>,
    #[serde(rename = "type")]
    kind: Option<String>,
}

fn decode_envelope<T>(response: reqwest::blocking::Response) -> Result<T>
where
    T: DeserializeOwned,
{
    let status = response.status();
    let url = response.url().clone();
    let body = response.text().map_err(Error::from)?;

    tracing::trace!(
        status = status.as_u16(),
        url = %url,
        body = %redact_presigned(&body),
        "HTTP response"
    );

    if let Ok(auth_err) = serde_json::from_str::<AuthErrorEnvelope>(&body) {
        let server_msg = auth_err
            .error
            .message
            .filter(|m| !m.trim().is_empty())
            .unwrap_or_else(|| "unauthorized".into());
        let kind = auth_err
            .error
            .kind
            .filter(|k| !k.trim().is_empty())
            .unwrap_or_else(|| "authentication_error".into());
        if matches!(status, StatusCode::UNAUTHORIZED | StatusCode::FORBIDDEN)
            || kind.contains("auth")
        {
            return Err(Error::Api {
                message: format!(
                    "API key was rejected by the server ({kind}): {server_msg}\n{}",
                    format_http_response(status, &body)
                ),
            });
        }
    }

    let envelope: ApiEnvelope = match serde_json::from_str(&body) {
        Ok(envelope) => envelope,
        Err(err) => {
            return Err(Error::Api {
                message: format!(
                    "unexpected response from {url} (expected MemoryLake API envelope with `success`; JSON error: {err})\n{}",
                    format_http_response(status, &body)
                ),
            });
        }
    };

    if !status.is_success() || !envelope.success {
        let code = envelope
            .error_code
            .filter(|c| !c.trim().is_empty())
            .map(|c| format!(" [{c}]"))
            .unwrap_or_default();
        let message = envelope
            .message
            .filter(|m| !m.trim().is_empty())
            .unwrap_or_else(|| "request failed".into());
        return Err(Error::Api {
            message: format!("{message}{code}\n{}", format_http_response(status, &body)),
        });
    }

    let data = envelope.data.unwrap_or(Value::Null);
    serde_json::from_value(data).map_err(|source| Error::Api {
        message: format!(
            "unexpected API data from {url}: {source}\n{}",
            format_http_response(status, &body)
        ),
    })
}

fn format_http_response(status: StatusCode, body: &str) -> String {
    const MAX_BODY: usize = 2_048;

    let body = redact_presigned(body.trim());
    let body = if body.is_empty() {
        "(empty body)".to_string()
    } else if body.len() > MAX_BODY {
        // Bodies carry user-supplied names, so the cut can land mid-codepoint.
        let mut end = MAX_BODY;
        while !body.is_char_boundary(end) {
            end -= 1;
        }
        format!("{}…", &body[..end])
    } else {
        body.into_owned()
    };
    format!("HTTP {status}\n{body}")
}

/// Query parameters that turn a URL into a replayable capability.
///
/// Pre-signed storage URLs reach us in two places: `upload_url` in a
/// create-upload response, and `x_attrs` entries such as thumbnail links on
/// ordinary items. Both end up in trace logs and error messages.
const CREDENTIAL_QUERY_PARAMS: [&str; 3] = [
    "x-amz-signature",
    "x-amz-credential",
    "x-amz-security-token",
];

/// Blank out the values of credential-bearing query parameters in `text`.
///
/// Operates on arbitrary text, not just URLs, because these links arrive
/// embedded in JSON response bodies. Returns the input untouched when it holds
/// nothing sensitive.
fn redact_presigned(text: &str) -> Cow<'_, str> {
    // ASCII-lowercasing preserves byte length, so indices map back to `text`.
    let lower = text.to_ascii_lowercase();
    if !lower.contains("x-amz-") {
        return Cow::Borrowed(text);
    }

    let mut out = String::with_capacity(text.len());
    let mut cursor = 0;
    while cursor < text.len() {
        let Some((start, name)) = CREDENTIAL_QUERY_PARAMS
            .iter()
            .filter_map(|param| lower[cursor..].find(param).map(|at| (cursor + at, *param)))
            .min_by_key(|(at, _)| *at)
        else {
            break;
        };

        let after_name = start + name.len();
        if !lower[after_name..].starts_with('=') {
            // A prefix match that is not this parameter; step past it.
            out.push_str(&text[cursor..after_name]);
            cursor = after_name;
            continue;
        }

        let value_start = after_name + 1;
        let value_end = text[value_start..]
            .find(|c: char| c == '&' || c == '"' || c == '\'' || c.is_whitespace())
            .map(|at| value_start + at)
            .unwrap_or(text.len());

        out.push_str(&text[cursor..value_start]);
        out.push_str("REDACTED");
        cursor = value_end;
    }
    out.push_str(&text[cursor..]);
    Cow::Owned(out)
}

/// Render headers for trace logs, redacting the `Authorization` value.
fn redact_headers(headers: &HeaderMap) -> BTreeMap<String, String> {
    headers
        .iter()
        .map(|(name, value)| {
            let raw = value.to_str().unwrap_or("<non-utf8>");
            let value_str = if name.as_str().eq_ignore_ascii_case("authorization") {
                mask_auth_value(raw)
            } else {
                raw.to_string()
            };
            (name.as_str().to_string(), value_str)
        })
        .collect()
}

/// Mask an Authorization header value while preserving the scheme and the last
/// 4 characters of the credential, e.g. `Bearer ******wxyz`. Credentials of
/// 4 chars or fewer are masked entirely so we never expose the whole secret.
fn mask_auth_value(value: &str) -> String {
    let (prefix, secret) = match value.split_once(' ') {
        Some((scheme, rest)) => (format!("{scheme} "), rest),
        None => (String::new(), value),
    };
    let n = secret.chars().count();
    if n <= 4 {
        return format!("{prefix}{}", "*".repeat(n));
    }
    let tail: String = secret.chars().skip(n - 4).collect();
    format!("{prefix}******{tail}")
}

/// Best-effort string view of a request body for trace logs.
fn request_body_as_str(request: &reqwest::blocking::Request) -> String {
    match request.body().and_then(|b| b.as_bytes()) {
        Some(bytes) => String::from_utf8_lossy(bytes).into_owned(),
        None => String::new(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    use crate::test_support::{json_ok, one_shot_server};

    #[test]
    fn library_item_id_colon_survives_into_the_request_line() {
        let (base, server) = one_shot_server(json_ok(
            r#"{"success":true,"message":"Item deleted successfully"}"#,
        ));
        let client = Client::new(base, "sk_test_key_abcdefghij").unwrap();

        client
            .delete_data::<()>("/api/v1/drives/items/sc-a:inode-b")
            .expect("delete envelope without data decodes");

        // `encode_segment` leaving `:` alone is only half the story: the URL
        // parser inside the HTTP stack could still normalize it. Assert on the
        // bytes that actually go out.
        let request = server.join().expect("server thread");
        assert!(
            request
                .head
                .starts_with("DELETE /api/v1/drives/items/sc-a:inode-b "),
            "colon must reach the wire unencoded:\n{}",
            request.head
        );
        assert!(request.has_header("authorization"));
    }

    #[test]
    fn part_upload_sends_no_authorization_and_returns_etag() {
        let (base, server) = one_shot_server(
            "HTTP/1.1 200 OK\r\nETag: \"ef370f8d0a3551d387b27728c34c5906\"\r\n\
             Content-Length: 0\r\n\r\n",
        );
        let client = Client::new("http://unused.invalid", "sk_test_key_abcdefghij").unwrap();

        let etag = client
            .put_presigned_part(
                &format!("{base}/bucket/part?X-Amz-Signature=deadbeef"),
                Body::from(vec![1u8, 2, 3, 4]),
            )
            .expect("part upload succeeds");

        // ETag is passed through verbatim, quotes included: the finalize
        // endpoint accepts it either way, and echoing what storage returned is
        // the form least likely to break.
        assert_eq!(etag, "\"ef370f8d0a3551d387b27728c34c5906\"");

        let request = server.join().expect("server thread");
        assert!(
            !request.has_header("authorization"),
            "pre-signed part upload must not carry credentials:\n{}",
            request.head
        );
        assert!(
            request
                .head
                .starts_with("PUT /bucket/part?X-Amz-Signature=deadbeef ")
        );
        assert!(
            request
                .head
                .to_ascii_lowercase()
                .contains("content-type: application/octet-stream")
        );
        assert_eq!(request.body, vec![1u8, 2, 3, 4]);
    }

    #[test]
    fn part_upload_classifies_retryable_statuses() {
        let (base, server) = one_shot_server(
            "HTTP/1.1 503 Service Unavailable\r\nContent-Length: 9\r\n\r\nslow down",
        );
        let client = Client::new("http://unused.invalid", "sk_test_key_abcdefghij").unwrap();

        let err = client
            .put_presigned_part(&format!("{base}/p"), Body::from(vec![0u8]))
            .expect_err("503 is an error");
        assert!(err.is_retryable());
        assert_eq!(err.status(), Some(StatusCode::SERVICE_UNAVAILABLE));
        let _ = server.join();
    }

    #[test]
    fn part_upload_treats_expired_url_as_terminal() {
        let (base, server) =
            one_shot_server("HTTP/1.1 403 Forbidden\r\nContent-Length: 16\r\n\r\nRequest expired.");
        let client = Client::new("http://unused.invalid", "sk_test_key_abcdefghij").unwrap();

        let err = client
            .put_presigned_part(&format!("{base}/p"), Body::from(vec![0u8]))
            .expect_err("403 is an error");
        // Retrying a fixed signature cannot help; only a new session can.
        assert!(!err.is_retryable());
        assert_eq!(err.status(), Some(StatusCode::FORBIDDEN));
        let _ = server.join();
    }

    #[test]
    fn part_upload_rejects_response_without_etag() {
        let (base, server) = one_shot_server("HTTP/1.1 200 OK\r\nContent-Length: 0\r\n\r\n");
        let client = Client::new("http://unused.invalid", "sk_test_key_abcdefghij").unwrap();

        let err = client
            .put_presigned_part(&format!("{base}/p"), Body::from(vec![0u8]))
            .expect_err("missing ETag is an error");
        assert!(matches!(err, PartUploadError::MissingETag));
        assert!(!err.is_retryable());
        let _ = server.join();
    }

    #[test]
    fn redact_presigned_masks_signature_and_credential() {
        let url = "https://s3.amazonaws.com/b/k?X-Amz-Algorithm=AWS4-HMAC-SHA256\
                   &X-Amz-Credential=AKIAEXAMPLE%2F20260806%2Fus-east-1%2Fs3%2Faws4_request\
                   &X-Amz-Expires=17999&X-Amz-Signature=7278c7e23a69cab27c3256ed08fc9e50";
        let redacted = redact_presigned(url);
        assert!(redacted.contains("X-Amz-Credential=REDACTED&"));
        assert!(redacted.ends_with("X-Amz-Signature=REDACTED"));
        // Non-credential parameters stay readable so traces remain useful.
        assert!(redacted.contains("X-Amz-Algorithm=AWS4-HMAC-SHA256"));
        assert!(redacted.contains("X-Amz-Expires=17999"));
        assert!(!redacted.contains("AKIAEXAMPLE"));
    }

    #[test]
    fn redact_presigned_handles_urls_embedded_in_json() {
        // Thumbnail links arrive inside `x_attrs` on ordinary list responses.
        let body = r#"{"x_attrs":{"x_thumbnail_uri":"https://s3/x?X-Amz-Signature=abc123"},"n":1}"#;
        let redacted = redact_presigned(body);
        assert_eq!(
            redacted,
            r#"{"x_attrs":{"x_thumbnail_uri":"https://s3/x?X-Amz-Signature=REDACTED"},"n":1}"#
        );
    }

    #[test]
    fn redact_presigned_leaves_ordinary_text_borrowed() {
        let body = r#"{"success":true,"data":{"item_id":"sc-a:inode-b"}}"#;
        assert!(matches!(redact_presigned(body), Cow::Borrowed(_)));
    }

    #[test]
    fn format_http_response_truncates_on_a_char_boundary() {
        // Item names are user-supplied, so a byte-indexed cut can land inside a
        // multi-byte codepoint.
        let body = format!("{}示例工作表.xlsx", "x".repeat(2_046));
        let formatted = format_http_response(StatusCode::OK, &body);
        assert!(formatted.ends_with('…'));
    }

    #[test]
    fn mask_auth_value_keeps_scheme_and_last_four() {
        assert_eq!(
            mask_auth_value("Bearer sk-abc123def4567890"),
            "Bearer ******7890"
        );
    }

    #[test]
    fn mask_auth_value_masks_short_secret_completely() {
        assert_eq!(mask_auth_value("Bearer abcd"), "Bearer ****");
        assert_eq!(mask_auth_value("Bearer a"), "Bearer *");
    }

    #[test]
    fn mask_auth_value_handles_missing_scheme() {
        assert_eq!(mask_auth_value("sk-abcdef123456"), "******3456");
    }

    #[test]
    fn redact_headers_masks_authorization_and_preserves_others() {
        let mut headers = HeaderMap::new();
        headers.insert(
            AUTHORIZATION,
            HeaderValue::from_static("Bearer sk-abcd1234567890"),
        );
        headers.insert(CONTENT_TYPE, HeaderValue::from_static("application/json"));

        let redacted = redact_headers(&headers);
        assert_eq!(redacted["authorization"], "Bearer ******7890");
        assert_eq!(redacted["content-type"], "application/json");
    }

    #[test]
    fn redact_headers_marks_non_utf8_values() {
        let mut headers = HeaderMap::new();
        // 0xC3 is a valid HTTP header byte but not printable ASCII, so
        // `HeaderValue::to_str` fails and we fall back to the placeholder.
        let value = HeaderValue::from_bytes(&[0xC3]).unwrap();
        headers.insert("x-binary", value);

        let redacted = redact_headers(&headers);
        assert_eq!(redacted["x-binary"], "<non-utf8>");
    }

    #[test]
    fn request_body_as_str_returns_json_body_bytes() {
        let http = reqwest::blocking::Client::new();
        let request = http
            .post("http://example.invalid/")
            .json(&serde_json::json!({"k": "v"}))
            .build()
            .unwrap();
        assert_eq!(request_body_as_str(&request), r#"{"k":"v"}"#);
    }

    #[test]
    fn request_body_as_str_empty_when_no_body() {
        let http = reqwest::blocking::Client::new();
        let request = http.get("http://example.invalid/").build().unwrap();
        assert!(request_body_as_str(&request).is_empty());
    }

    #[test]
    fn trace_calls_evaluate_at_trace_level() {
        use std::net::TcpListener;
        use tracing::subscriber::with_default;
        use tracing_subscriber::fmt;

        let subscriber = fmt::Subscriber::builder()
            .with_max_level(tracing::Level::TRACE)
            .with_test_writer()
            .finish();

        with_default(subscriber, || {
            // Reserve then release a loopback port so `execute()` fails fast
            // with connection refused — but only after `trace!` args run.
            let listener = TcpListener::bind("127.0.0.1:0").unwrap();
            let addr = listener.local_addr().unwrap();
            drop(listener);

            let client = Client::new(format!("http://{addr}"), "test-key-abcdefghij1234").unwrap();

            let _ = client.get_data::<serde_json::Value>("/x", &[("k", "v".to_string())]);
            let _ = client.post_data::<serde_json::Value, _>("/x", &serde_json::json!({"a": 1}));
            let _ = client.patch_data::<serde_json::Value, _>("/x", &serde_json::json!({"a": 1}));
            let _ = client.delete_data::<()>("/x");
        });
    }

    #[test]
    fn patch_and_delete_mask_authorization_in_traces() {
        use std::net::TcpListener;
        use tracing::subscriber::with_default;
        use tracing_subscriber::fmt;

        let log = SharedBuf::default();
        let subscriber = fmt::Subscriber::builder()
            .with_max_level(tracing::Level::TRACE)
            .with_writer({
                let log = log.clone();
                move || log.clone()
            })
            .finish();

        with_default(subscriber, || {
            // Dead loopback port: the request fails after `trace!` has already
            // rendered its fields, which is exactly what we want to inspect.
            let listener = TcpListener::bind("127.0.0.1:0").unwrap();
            let addr = listener.local_addr().unwrap();
            drop(listener);

            let client = Client::new(format!("http://{addr}"), "sk_supersecret_7890").unwrap();
            let _ = client.patch_data::<serde_json::Value, _>("/x", &serde_json::json!({"a": 1}));
            let _ = client.delete_data::<()>("/x");
        });

        let logged = log.contents();
        assert!(logged.contains("PATCH"), "PATCH not traced: {logged}");
        assert!(logged.contains("DELETE"), "DELETE not traced: {logged}");
        assert!(
            logged.contains("Bearer ******7890"),
            "authorization not masked: {logged}"
        );
        assert!(
            !logged.contains("sk_supersecret_7890"),
            "raw API key leaked into trace output: {logged}"
        );
    }

    #[test]
    fn delete_accepts_success_envelope_without_data() {
        let server = StubServer::new(
            "200 OK",
            r#"{"success":true,"message":"Operation completed successfully"}"#,
        );
        let client = Client::new(&server.base_url, "sk_test_key_1234").unwrap();

        client
            .delete_data::<()>("/api/v3/actors/act-1")
            .expect("empty-data envelope should decode into ()");

        let request = server.received();
        assert!(
            request.starts_with("DELETE /api/v3/actors/act-1 "),
            "unexpected request line: {request}"
        );
    }

    #[test]
    fn patch_sends_json_body_and_decodes_data() {
        let server = StubServer::new("200 OK", r#"{"success":true,"data":{"id":"act-1"}}"#);
        let client = Client::new(&server.base_url, "sk_test_key_1234").unwrap();

        let data: Value = client
            .patch_data(
                "/api/v3/actors/act-1",
                &serde_json::json!({"display_name": "Alice"}),
            )
            .expect("patch should decode data");
        assert_eq!(data["id"], "act-1");

        let request = server.received();
        assert!(
            request.starts_with("PATCH /api/v3/actors/act-1 "),
            "unexpected request line: {request}"
        );
        assert!(
            request.contains(r#"{"display_name":"Alice"}"#),
            "body not sent: {request}"
        );
    }

    #[test]
    fn empty_data_endpoints_still_fail_on_unsuccessful_envelope() {
        let server = StubServer::new(
            "200 OK",
            r#"{"success":false,"message":"actor not found","error_code":"ACTOR_NOT_FOUND"}"#,
        );
        let client = Client::new(&server.base_url, "sk_test_key_1234").unwrap();

        let err = client
            .delete_data::<()>("/api/v3/actors/missing")
            .expect_err("success:false must not decode as an empty-data success");
        let message = err.to_string();
        assert!(message.contains("actor not found"), "{message}");
        assert!(message.contains("[ACTOR_NOT_FOUND]"), "{message}");
    }

    #[test]
    fn payload_endpoints_still_reject_missing_data() {
        // Accepting an absent `data` for delete must not loosen decoding for
        // endpoints that are supposed to return a payload.
        #[derive(Debug, serde::Deserialize)]
        struct Payload {
            #[allow(dead_code)] // only its absence is under test
            id: String,
        }

        let server = StubServer::new("200 OK", r#"{"success":true}"#);
        let client = Client::new(&server.base_url, "sk_test_key_1234").unwrap();

        let err = client
            .get_data::<Payload>("/api/v3/actors/act-1", &[])
            .expect_err("a payload endpoint must reject a missing data field");
        assert!(
            err.to_string().contains("unexpected API data"),
            "{}",
            err.to_string()
        );
    }

    /// Shared in-memory sink so a test can assert on rendered trace output.
    #[derive(Clone, Default)]
    struct SharedBuf(std::sync::Arc<std::sync::Mutex<Vec<u8>>>);

    impl SharedBuf {
        fn contents(&self) -> String {
            let guard = self.0.lock().unwrap_or_else(|err| err.into_inner());
            String::from_utf8_lossy(&guard).into_owned()
        }
    }

    impl std::io::Write for SharedBuf {
        fn write(&mut self, buf: &[u8]) -> std::io::Result<usize> {
            let mut guard = self.0.lock().unwrap_or_else(|err| err.into_inner());
            guard.extend_from_slice(buf);
            Ok(buf.len())
        }

        fn flush(&mut self) -> std::io::Result<()> {
            Ok(())
        }
    }

    /// One-shot loopback HTTP server: serves a single canned response and hands
    /// the raw request text back so tests can assert on method, path, and body.
    struct StubServer {
        base_url: String,
        requests: std::sync::mpsc::Receiver<String>,
        handle: Option<std::thread::JoinHandle<()>>,
    }

    impl StubServer {
        fn new(status_line: &str, body: &str) -> Self {
            use std::net::TcpListener;

            let listener = TcpListener::bind("127.0.0.1:0").expect("bind stub server");
            let addr = listener.local_addr().expect("stub server address");
            let response = format!(
                "HTTP/1.1 {status_line}\r\ncontent-type: application/json\r\ncontent-length: {}\r\nconnection: close\r\n\r\n{body}",
                body.len()
            );

            let (sender, requests) = std::sync::mpsc::channel();
            let handle = std::thread::spawn(move || {
                use std::io::Write;

                let Ok((mut stream, _)) = listener.accept() else {
                    return;
                };
                let request = read_http_request(&mut stream);
                // Best effort: if the receiver is gone the test already ended.
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
            self.requests
                .recv_timeout(std::time::Duration::from_secs(10))
                .expect("stub server should have received a request")
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
    fn read_http_request(stream: &mut std::net::TcpStream) -> String {
        use std::io::Read;

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
}
