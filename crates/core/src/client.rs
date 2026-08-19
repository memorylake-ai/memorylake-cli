//! Blocking HTTP client for the MemoryLake OpenAPI.

use std::borrow::Cow;
use std::collections::BTreeMap;

use reqwest::blocking::{Body, Client as HttpClient};
use reqwest::header::{
    AUTHORIZATION, CONTENT_DISPOSITION, CONTENT_TYPE, ETAG, HeaderMap, HeaderValue,
};
use reqwest::{StatusCode, Url};
use serde::Serialize;
use serde::de::DeserializeOwned;
use serde_json::Value;

use crate::error::{Error, Result};

/// What a completed download reported about itself.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Downloaded {
    /// Name the server suggested, already reduced to a bare file name.
    ///
    /// `None` when the server sent no usable `Content-Disposition`; the caller
    /// then has to name the file itself.
    pub filename: Option<String>,
    /// `Content-Type` the server reported, if any.
    pub content_type: Option<String>,
    /// Bytes written to the destination.
    pub bytes: u64,
}

/// Extract a file name from a `Content-Disposition` header value.
///
/// Prefers RFC 5987's `filename*` over plain `filename`, because only the
/// former can carry non-ASCII names; the servers observed send both.
///
/// The result is always reduced to a bare file name. The value is chosen by the
/// server, and it lands on the caller's disk: `../../.ssh/authorized_keys` is a
/// path traversal, and an absolute path is an overwrite of the server's
/// choosing. Anything that still looks like a path after stripping components
/// yields `None` rather than a guess, leaving the caller to name the file.
fn filename_from_content_disposition(header: &str) -> Option<String> {
    let mut fallback = None;

    for part in header.split(';') {
        let part = part.trim();
        let Some((key, value)) = part.split_once('=') else {
            continue;
        };
        let key = key.trim().to_ascii_lowercase();
        let value = value.trim().trim_matches('"');

        if key == "filename*" {
            // `UTF-8''name%20with%20spaces` — charset and language are dropped;
            // the percent-encoded name is what matters.
            let encoded = value
                .rsplit_once("''")
                .map(|(_, name)| name)
                .unwrap_or(value);
            if let Some(name) = sanitize_filename(&percent_decode(encoded)) {
                return Some(name);
            }
        } else if key == "filename" {
            fallback = sanitize_filename(value);
        }
    }

    fallback
}

/// Reduce a server-supplied name to a bare, safe file name.
fn sanitize_filename(raw: &str) -> Option<String> {
    let name = raw.trim();
    if name.is_empty() {
        return None;
    }

    // Take the last segment under either separator: a Windows client must not
    // trust a `\` any more than a Unix one trusts `/`.
    let name = name
        .rsplit(['/', '\\'])
        .next()
        .map(str::trim)
        .unwrap_or_default();

    // `.` and `..` name directories, and a leading NUL or control character has
    // no business in a file name.
    if name.is_empty()
        || name == "."
        || name == ".."
        || name.contains('\0')
        || name.chars().any(char::is_control)
    {
        return None;
    }

    Some(name.to_string())
}

/// Decode percent-escapes in a `filename*` value.
///
/// Deliberately small: this decodes one header field, not arbitrary URLs, and
/// invalid escapes are left alone rather than failing the download.
fn percent_decode(value: &str) -> String {
    let bytes = value.as_bytes();
    let mut out = Vec::with_capacity(bytes.len());
    let mut index = 0;

    while index < bytes.len() {
        if bytes[index] == b'%' && index + 2 < bytes.len() {
            let hex = std::str::from_utf8(&bytes[index + 1..index + 3]).ok();
            if let Some(byte) = hex.and_then(|hex| u8::from_str_radix(hex, 16).ok()) {
                out.push(byte);
                index += 3;
                continue;
            }
        }
        out.push(bytes[index]);
        index += 1;
    }

    String::from_utf8_lossy(&out).into_owned()
}

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

    /// Stream a binary response body into `writer`.
    ///
    /// Unlike every other method here, the success path is not a JSON envelope
    /// — it is the file itself — so the body is copied out in chunks rather
    /// than buffered and parsed. Errors still arrive as envelopes, and are
    /// decoded the usual way.
    ///
    /// Redirects are followed, which this endpoint requires: the API answers
    /// `303` pointing at storage. `reqwest` strips `Authorization` when a
    /// redirect crosses to another host, so the MemoryLake key is never handed
    /// to the storage provider — which needs no such thing, the signature being
    /// in the URL.
    pub fn download_to<W: std::io::Write>(&self, path: &str, writer: &mut W) -> Result<Downloaded> {
        let url = self.url(path);
        let request = self.http.get(&url).headers(self.auth_headers()?).build()?;
        let mut response = self.execute(request)?;

        let status = response.status();
        // The final URL after redirects is a pre-signed storage link whose
        // query string is a working credential; it must never be logged as-is.
        let final_url = redact_presigned(response.url().as_str()).into_owned();
        tracing::trace!(status = status.as_u16(), url = %final_url, "download response");

        if !status.is_success() {
            // An error is an ordinary envelope, so hand it to the shared
            // decoder for a message consistent with the rest of the API. It
            // always errors on a non-success status; the other arm exists so
            // that invariant cannot turn into a panic.
            return match validate_envelope(response) {
                Err(err) => Err(err),
                Ok(_) => Err(Error::Api {
                    message: format!("download failed with status {status}"),
                    code: None,
                }),
            };
        }

        let filename = response
            .headers()
            .get(CONTENT_DISPOSITION)
            .and_then(|value| value.to_str().ok())
            .and_then(filename_from_content_disposition);
        let content_type = response
            .headers()
            .get(CONTENT_TYPE)
            .and_then(|value| value.to_str().ok())
            .map(str::to_string);

        let bytes = std::io::copy(&mut response, writer).map_err(|source| Error::Io {
            action: "write downloaded content",
            path: std::path::PathBuf::from("<writer>"),
            source,
        })?;

        Ok(Downloaded {
            filename,
            content_type,
            bytes,
        })
    }

    /// Perform a DELETE whose successful response carries no usable payload.
    ///
    /// Prefer this over `delete_data::<()>` for endpoints documented to answer
    /// `{"success": true, "data": {}}`: an empty JSON *object* cannot
    /// deserialize into `()`, so the unit form would reject a perfectly good
    /// response. This variant validates the envelope — a non-2xx status or
    /// `success: false` is still an error — and discards whatever `data` holds.
    pub fn delete_empty(&self, path: &str) -> Result<()> {
        let url = self.url(path);
        let request = self
            .http
            .delete(&url)
            .headers(self.auth_headers()?)
            .build()?;
        validate_envelope(self.execute(request)?)?;
        Ok(())
    }

    /// Perform a DELETE that carries a JSON body and returns no usable payload.
    ///
    /// Some collection endpoints name their targets in the body rather than the
    /// path — removing documents from a project, for instance. `reqwest` allows
    /// a body on DELETE, but no other delete helper here sends one, so this is a
    /// separate method rather than an extra argument threaded through them.
    ///
    /// Envelope handling matches [`Self::delete_empty`]: a non-2xx status or
    /// `success: false` is an error, and whatever `data` holds is discarded.
    pub fn delete_empty_with_body<B>(&self, path: &str, body: &B) -> Result<()>
    where
        B: Serialize,
    {
        let url = self.url(path);
        let request = self
            .http
            .delete(&url)
            .headers(self.auth_headers()?)
            .json(body)
            .build()?;
        validate_envelope(self.execute(request)?)?;
        Ok(())
    }

    /// Trace, execute, and decode a prepared request.
    fn send<T>(&self, request: reqwest::blocking::Request) -> Result<T>
    where
        T: DeserializeOwned,
    {
        decode_envelope(self.execute(request)?)
    }

    /// Trace and execute a prepared request.
    ///
    /// Every verb funnels through here so the `Authorization` header is
    /// redacted in exactly one place and no method can bypass it.
    fn execute(&self, request: reqwest::blocking::Request) -> Result<reqwest::blocking::Response> {
        tracing::trace!(
            method = %request.method(),
            url = %request.url(),
            headers = ?redact_headers(request.headers()),
            body = %request_body_as_str(&request),
            "HTTP request"
        );
        Ok(self.http.execute(request)?)
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
                code: None,
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

/// Status, URL, and raw body of a response, retained for error messages.
struct ResponseContext {
    status: StatusCode,
    url: Url,
    body: String,
}

/// Validate the MemoryLake envelope and return its `data` payload.
///
/// Errors on auth-gateway error bodies, unparseable bodies, non-2xx statuses,
/// and `success: false`. A missing `data` key yields [`Value::Null`]. Callers
/// that expect no payload discard the value; [`decode_envelope`] deserializes it.
fn validate_envelope(response: reqwest::blocking::Response) -> Result<(Value, ResponseContext)> {
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
                code: None,
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
                code: None,
            });
        }
    };

    if !status.is_success() || !envelope.success {
        let code = envelope.error_code.filter(|c| !c.trim().is_empty());
        let code_suffix = code
            .as_deref()
            .map(|c| format!(" [{c}]"))
            .unwrap_or_default();
        let message = envelope
            .message
            .filter(|m| !m.trim().is_empty())
            .unwrap_or_else(|| "request failed".into());
        return Err(Error::Api {
            message: format!(
                "{message}{code_suffix}\n{}",
                format_http_response(status, &body)
            ),
            code,
        });
    }

    let data = envelope.data.unwrap_or(Value::Null);
    Ok((data, ResponseContext { status, url, body }))
}

/// Validate the envelope and deserialize its `data` payload into `T`.
fn decode_envelope<T>(response: reqwest::blocking::Response) -> Result<T>
where
    T: DeserializeOwned,
{
    let (data, ctx) = validate_envelope(response)?;
    serde_json::from_value(data).map_err(|source| Error::Api {
        message: format!(
            "unexpected API data from {}: {source}\n{}",
            ctx.url,
            format_http_response(ctx.status, &ctx.body)
        ),
        code: None,
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
const CREDENTIAL_QUERY_PARAMS: [&str; 5] = [
    // SigV4, used by the upload URLs.
    "x-amz-signature",
    "x-amz-credential",
    "x-amz-security-token",
    // SigV2, used by the document download redirect. Different scheme, same
    // consequence if it reaches a log: `Signature` plus `AWSAccessKeyId` is a
    // working credential until `Expires` passes.
    //
    // `signature` also occurs inside `x-amz-signature`; the scan takes the
    // earliest match, so the longer name still wins there.
    "awsaccesskeyid",
    "signature",
];

/// Blank out the values of credential-bearing query parameters in `text`.
///
/// Operates on arbitrary text, not just URLs, because these links arrive
/// embedded in JSON response bodies. Returns the input untouched when it holds
/// nothing sensitive.
fn redact_presigned(text: &str) -> Cow<'_, str> {
    // ASCII-lowercasing preserves byte length, so indices map back to `text`.
    let lower = text.to_ascii_lowercase();
    if !CREDENTIAL_QUERY_PARAMS
        .iter()
        .any(|param| lower.contains(param))
    {
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
    fn content_disposition_yields_the_measured_filename() {
        // Exactly what the download endpoint sent on 2026-08-19.
        let header = "attachment; filename=\"payload.txt\"; filename*=UTF-8''payload.txt";
        assert_eq!(
            filename_from_content_disposition(header),
            Some("payload.txt".to_string())
        );
    }

    #[test]
    fn content_disposition_prefers_the_encoded_form() {
        // Only `filename*` can carry non-ASCII, so it wins when both are sent
        // and disagree.
        let header = "attachment; filename=\"report.pdf\"; filename*=UTF-8''%E6%8A%A5%E5%91%8A.pdf";
        assert_eq!(
            filename_from_content_disposition(header),
            Some("报告.pdf".to_string())
        );
    }

    #[test]
    fn content_disposition_falls_back_to_the_plain_form() {
        assert_eq!(
            filename_from_content_disposition("attachment; filename=notes.md"),
            Some("notes.md".to_string())
        );
        assert_eq!(
            filename_from_content_disposition("attachment; filename=\"with spaces.txt\""),
            Some("with spaces.txt".to_string())
        );
    }

    #[test]
    fn content_disposition_without_a_name_yields_nothing() {
        assert_eq!(filename_from_content_disposition("attachment"), None);
        assert_eq!(filename_from_content_disposition("inline"), None);
        assert_eq!(filename_from_content_disposition(""), None);
        assert_eq!(
            filename_from_content_disposition("attachment; filename=\"\""),
            None
        );
    }

    #[test]
    fn a_server_supplied_name_cannot_escape_the_target_directory() {
        // The name is chosen by the server and used to create a file. A
        // traversal or an absolute path would let it pick the location.
        for header in [
            r#"attachment; filename="../../.ssh/authorized_keys""#,
            r#"attachment; filename="/etc/passwd""#,
            r#"attachment; filename="..\..\Windows\System32\evil.dll""#,
            r#"attachment; filename*=UTF-8''..%2F..%2Fescaped.txt"#,
        ] {
            let name = filename_from_content_disposition(header);
            let name = name.as_deref().unwrap_or_default();
            assert!(
                !name.contains('/') && !name.contains('\\'),
                "{header} produced a path: {name:?}"
            );
            assert!(name != ".." && name != ".", "{header} produced {name:?}");
        }
    }

    #[test]
    fn a_traversal_keeps_the_harmless_tail() {
        // Stripping directories leaves a usable name; only the location is
        // rejected, not the download.
        assert_eq!(
            filename_from_content_disposition(r#"attachment; filename="../../report.pdf""#),
            Some("report.pdf".to_string())
        );
    }

    #[test]
    fn a_name_that_is_only_a_path_yields_nothing() {
        // Nothing usable is left after stripping, so the caller must name it.
        assert_eq!(
            filename_from_content_disposition(r#"attachment; filename="../..""#),
            None
        );
        assert_eq!(
            filename_from_content_disposition(r#"attachment; filename="/""#),
            None
        );
    }

    #[test]
    fn control_characters_are_rejected() {
        assert_eq!(
            filename_from_content_disposition("attachment; filename=\"bad\nname.txt\""),
            None
        );
    }

    #[test]
    fn percent_decoding_leaves_invalid_escapes_alone() {
        // A malformed escape must not fail the download; the name is a
        // convenience, not a contract.
        assert_eq!(percent_decode("100%off.txt"), "100%off.txt");
        assert_eq!(percent_decode("a%zz.txt"), "a%zz.txt");
        assert_eq!(percent_decode("ok%20name.txt"), "ok name.txt");
    }

    #[test]
    fn redact_presigned_masks_the_sigv2_download_redirect() {
        // Shape measured 2026-08-19: the document download 303 points at a
        // SigV2 URL, which carries no `X-Amz-` parameter at all. `Signature`
        // plus `AWSAccessKeyId` is a usable credential until `Expires` passes,
        // so neither may reach a log.
        let url = "https://bloom-s3.s3.amazonaws.com/drive/d/sc-a/inode-b/1\
                   ?response-content-disposition=attachment%3B%20filename%3D%22payload.txt%22\
                   &AWSAccessKeyId=AKIARLSQLXURHEIDN4OZ&Signature=2zUYV0rOm6chhHldtDp1MYFmZm0%3D\
                   &Expires=1787120029";
        let redacted = redact_presigned(url);

        assert!(!redacted.contains("AKIARLSQLXURHEIDN4OZ"), "{redacted}");
        assert!(
            !redacted.contains("2zUYV0rOm6chhHldtDp1MYFmZm0"),
            "{redacted}"
        );
        assert!(redacted.contains("AWSAccessKeyId=REDACTED"), "{redacted}");
        assert!(redacted.contains("Signature=REDACTED"), "{redacted}");
        // Everything else stays readable, or the trace stops being useful.
        assert!(redacted.contains("Expires=1787120029"), "{redacted}");
        assert!(redacted.contains("filename"), "{redacted}");
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

    /// Serializes tests that install a scoped `tracing` subscriber.
    ///
    /// `tracing` keeps a process-wide max-level hint. When one test's scoped
    /// subscriber is torn down while another is still running, the second
    /// test's `trace!` call sites are skipped by the level fast path and it
    /// observes no output at all. Holding this lock keeps them from racing.
    fn trace_lock() -> &'static std::sync::Mutex<()> {
        static LOCK: std::sync::OnceLock<std::sync::Mutex<()>> = std::sync::OnceLock::new();
        LOCK.get_or_init(|| std::sync::Mutex::new(()))
    }

    /// Pin a process-wide TRACE dispatcher for the lifetime of the test binary.
    ///
    /// `tracing` derives a global max-level hint and a per-callsite interest
    /// cache from the set of *registered* dispatchers. When the only TRACE
    /// subscribers are scoped ones, tearing one down drops the hint back to
    /// OFF and marks the client's `trace!` callsites uninteresting — and a
    /// test asserting on trace output at that moment records nothing. The
    /// other tests in this module drive the client with no subscriber at all,
    /// so this happens often enough to fail under `cargo test`'s default
    /// parallelism. One permanently registered TRACE dispatcher that discards
    /// its output keeps the hint pinned; scoped subscribers installed by
    /// individual tests still take precedence for routing events.
    fn pin_global_trace_level() {
        use tracing_subscriber::fmt;
        static PINNED: std::sync::OnceLock<()> = std::sync::OnceLock::new();
        PINNED.get_or_init(|| {
            let subscriber = fmt::Subscriber::builder()
                .with_max_level(tracing::Level::TRACE)
                .with_writer(std::io::sink)
                .finish();
            // A global default may already exist; pinning is best-effort.
            let _ = tracing::subscriber::set_global_default(subscriber);
            tracing::callsite::rebuild_interest_cache();
        });
    }

    #[test]
    fn trace_calls_evaluate_at_trace_level() {
        use std::net::TcpListener;
        use tracing::subscriber::with_default;
        use tracing_subscriber::fmt;

        let _serialized = trace_lock().lock().unwrap_or_else(|err| err.into_inner());
        pin_global_trace_level();
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

        let _serialized = trace_lock().lock().unwrap_or_else(|err| err.into_inner());
        pin_global_trace_level();
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

    #[test]
    fn unit_delete_cannot_decode_an_empty_data_object() {
        // Why `delete_empty` exists next to `delete_data`. The Agents API
        // documents deletes as `{"success":true,"data":{}}`, and an empty JSON
        // object is not a unit value, so the `delete_data::<()>` form rejects a
        // response the server considers successful.
        let server = StubServer::new("200 OK", r#"{"success":true,"data":{}}"#);
        let client = Client::new(&server.base_url, "sk_test_key_1234").unwrap();

        let err = client
            .delete_data::<()>("/api/v3/agents/agt-1")
            .expect_err("`{}` must not decode into `()`");
        assert!(
            err.to_string().contains("unexpected API data"),
            "{}",
            err.to_string()
        );
    }

    #[test]
    fn delete_empty_accepts_an_empty_data_object() {
        let server = StubServer::new("200 OK", r#"{"success":true,"data":{}}"#);
        let client = Client::new(&server.base_url, "sk_test_key_1234").unwrap();

        client
            .delete_empty("/api/v3/agents/agt-1")
            .expect("an empty data object is a successful delete");
        assert!(server.received().starts_with("DELETE /api/v3/agents/agt-1"));
    }

    #[test]
    fn delete_empty_accepts_an_absent_data_field() {
        // What the live Agents API actually returns, despite the documented
        // `data: {}`. Both shapes must count as success.
        let server = StubServer::new("200 OK", r#"{"success":true}"#);
        let client = Client::new(&server.base_url, "sk_test_key_1234").unwrap();

        client
            .delete_empty("/api/v3/agents/agt-1")
            .expect("an absent data field is a successful delete");
    }

    #[test]
    fn delete_empty_rejects_an_unsuccessful_envelope() {
        let server = StubServer::new(
            "200 OK",
            r#"{"success":false,"message":"agent is in use","error_code":"CONFLICT"}"#,
        );
        let client = Client::new(&server.base_url, "sk_test_key_1234").unwrap();

        let message = client
            .delete_empty("/api/v3/agents/agt-1")
            .expect_err("success:false must not be swallowed")
            .to_string();
        assert!(message.contains("agent is in use"), "{message}");
        assert!(message.contains("[CONFLICT]"), "{message}");
    }

    #[test]
    fn delete_empty_rejects_a_non_success_status() {
        let server = StubServer::new(
            "404 Not Found",
            r#"{"success":false,"message":"agent not found"}"#,
        );
        let client = Client::new(&server.base_url, "sk_test_key_1234").unwrap();

        let message = client
            .delete_empty("/api/v3/agents/missing")
            .expect_err("404 must not be swallowed")
            .to_string();
        assert!(message.contains("agent not found"), "{message}");
        assert!(message.contains("404"), "{message}");
    }

    #[test]
    fn delete_with_body_puts_the_json_payload_on_the_wire() {
        // The whole reason this method exists: the ids identifying what to
        // delete travel in the body, so a bodyless DELETE would reach the
        // server with nothing to act on.
        let (base, server) = one_shot_server(json_ok(r#"{"success":true,"data":{}}"#));
        let client = Client::new(base, "sk_test_key_abcdefghij").unwrap();

        client
            .delete_empty_with_body(
                "/api/v3/workspaces/ws-1/projects/proj-1/memories/documents",
                &serde_json::json!({"ids": ["doc-a", "doc-b"]}),
            )
            .expect("delete with body succeeds");

        let request = server.join().expect("server thread");
        assert!(
            request
                .head
                .starts_with("DELETE /api/v3/workspaces/ws-1/projects/proj-1/memories/documents "),
            "unexpected request line:\n{}",
            request.head
        );
        assert_eq!(
            String::from_utf8_lossy(&request.body),
            r#"{"ids":["doc-a","doc-b"]}"#
        );
        // Routed through `execute`, so it carries credentials like every other
        // authenticated verb.
        assert!(request.has_header("authorization"));
    }

    #[test]
    fn delete_with_body_accepts_an_empty_data_object() {
        let server = StubServer::new("200 OK", r#"{"success":true,"data":{}}"#);
        let client = Client::new(&server.base_url, "sk_test_key_1234").unwrap();

        client
            .delete_empty_with_body("/api/v3/docs", &serde_json::json!({"ids": ["doc-a"]}))
            .expect("an empty data object is a successful delete");
    }

    #[test]
    fn delete_with_body_accepts_an_absent_data_field() {
        let server = StubServer::new("200 OK", r#"{"success":true}"#);
        let client = Client::new(&server.base_url, "sk_test_key_1234").unwrap();

        client
            .delete_empty_with_body("/api/v3/docs", &serde_json::json!({"ids": ["doc-a"]}))
            .expect("an absent data field is a successful delete");
    }

    #[test]
    fn delete_with_body_rejects_an_unsuccessful_envelope() {
        let server = StubServer::new(
            "200 OK",
            r#"{"success":false,"message":"document not found","error_code":"NOT_FOUND"}"#,
        );
        let client = Client::new(&server.base_url, "sk_test_key_1234").unwrap();

        let message = client
            .delete_empty_with_body("/api/v3/docs", &serde_json::json!({"ids": ["doc-a"]}))
            .expect_err("success:false must not be swallowed")
            .to_string();
        assert!(message.contains("document not found"), "{message}");
        assert!(message.contains("[NOT_FOUND]"), "{message}");
    }

    #[test]
    fn delete_with_body_rejects_a_non_success_status() {
        let server = StubServer::new(
            "403 Forbidden",
            r#"{"success":false,"message":"permission denied"}"#,
        );
        let client = Client::new(&server.base_url, "sk_test_key_1234").unwrap();

        let message = client
            .delete_empty_with_body("/api/v3/docs", &serde_json::json!({"ids": ["doc-a"]}))
            .expect_err("403 must not be swallowed")
            .to_string();
        assert!(message.contains("permission denied"), "{message}");
        assert!(message.contains("403"), "{message}");
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
