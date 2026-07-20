//! Blocking HTTP client for the MemoryLake OpenAPI.

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
        let mut request = self.http.get(&url).headers(self.auth_headers()?);
        for (key, value) in query {
            request = request.query(&[(key, value)]);
        }
        let response = request.send()?;
        decode_envelope(response)
    }

    /// Perform a POST with a JSON body and deserialize the API `data` payload.
    pub fn post_data<T, B>(&self, path: &str, body: &B) -> Result<T>
    where
        T: DeserializeOwned,
        B: Serialize,
    {
        let url = self.url(path);
        let response = self
            .http
            .post(&url)
            .headers(self.auth_headers()?)
            .json(body)
            .send()?;
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
