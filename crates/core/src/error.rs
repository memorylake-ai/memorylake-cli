//! Public error types for `memorylake-core`.

use std::path::PathBuf;

use thiserror::Error;

/// Convenient `Result` alias for core APIs.
pub type Result<T> = std::result::Result<T, Error>;

/// Errors returned by core library operations.
#[derive(Debug, Error)]
pub enum Error {
    /// Failed to locate the user home directory.
    #[error("could not determine home directory")]
    HomeDir,

    /// I/O failure while reading or writing a local file.
    #[error("failed to {action} {path}")]
    Io {
        /// Human-readable action (e.g. "read", "write").
        action: &'static str,
        /// Path involved in the failure.
        path: PathBuf,
        /// Underlying I/O error.
        #[source]
        source: std::io::Error,
    },

    /// Failed to parse or serialize TOML.
    #[error("invalid TOML in {path}: {source}")]
    Toml {
        /// Path of the TOML file.
        path: PathBuf,
        /// Underlying parse/serialize error.
        #[source]
        source: Box<dyn std::error::Error + Send + Sync>,
    },

    /// No active profile and none was requested.
    #[error("not logged in; run `memorylake auth login`")]
    NotLoggedIn,

    /// Referenced profile does not exist.
    #[error("unknown profile `{name}`")]
    UnknownProfile {
        /// Profile name that was not found.
        name: String,
    },

    /// Profile exists in config but has no stored API key.
    #[error("profile `{name}` has no API key; run `memorylake auth login`")]
    MissingApiKey {
        /// Profile missing credentials.
        name: String,
    },

    /// Profile credentials use a login method that cannot be resolved yet.
    #[error("profile `{name}` uses unsupported login method `{method}`")]
    UnsupportedLoginMethod {
        /// Profile name.
        name: String,
        /// Stored login method.
        method: String,
    },

    /// HTTP transport or protocol failure.
    #[error("{}", format_http_error(.0))]
    Http(#[from] reqwest::Error),

    /// API returned an error or an unexpected HTTP response body.
    #[error("{message}")]
    Api {
        /// Human-readable error message (may include HTTP status and body).
        message: String,
    },
}

fn format_http_error(err: &reqwest::Error) -> String {
    if err.is_timeout() {
        return "timed out connecting to MemoryLake API; check your network and base URL".into();
    }
    if err.is_connect() {
        return "could not connect to MemoryLake API; check your network and base URL".into();
    }
    format!("request to MemoryLake API failed: {err}")
}
