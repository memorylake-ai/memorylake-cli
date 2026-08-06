//! Blocking HTTP client for the MemoryLake OpenAPI.

use std::collections::BTreeMap;

use reqwest::StatusCode;
use reqwest::blocking::Client as HttpClient;
use reqwest::header::{AUTHORIZATION, CONTENT_TYPE, HeaderMap, HeaderValue};
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
        let request = builder.build()?;
        tracing::trace!(
            method = %request.method(),
            url = %request.url(),
            headers = ?redact_headers(request.headers()),
            "HTTP request"
        );
        let response = self.http.execute(request)?;
        decode_envelope(response)
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
        body = %body,
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
    let body = body.trim();
    let body = if body.is_empty() {
        "(empty body)".to_string()
    } else if body.len() > 2_048 {
        format!("{}…", &body[..2_048])
    } else {
        body.to_string()
    };
    format!("HTTP {status}\n{body}")
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
        });
    }
}
