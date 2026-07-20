//! Blocking HTTP client for the MemoryLake OpenAPI.

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
                code: String::new(),
                message: format!("invalid API key header value: {err}"),
            }
        })?;
        headers.insert(AUTHORIZATION, value);
        Ok(headers)
    }
}

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

fn decode_envelope<T>(response: reqwest::blocking::Response) -> Result<T>
where
    T: DeserializeOwned,
{
    let status = response.status();
    let envelope: ApiEnvelope = response.json()?;

    if !status.is_success() || !envelope.success {
        let code = envelope
            .error_code
            .map(|c| format!(" ({c})"))
            .unwrap_or_default();
        let message = envelope.message.unwrap_or_else(|| format!("HTTP {status}"));
        return Err(Error::Api { code, message });
    }

    let data = envelope.data.unwrap_or(Value::Null);
    serde_json::from_value(data).map_err(|source| Error::Api {
        code: String::new(),
        message: format!("failed to decode API data: {source}"),
    })
}
