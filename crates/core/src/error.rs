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
    #[error("not logged in; run `memorylake auth login api_key --api-key <KEY>`")]
    NotLoggedIn,

    /// Referenced profile does not exist.
    #[error("unknown profile `{name}`")]
    UnknownProfile {
        /// Profile name that was not found.
        name: String,
    },

    /// Profile exists in config but has no stored API key.
    #[error("profile `{name}` has no API key; run `memorylake auth login api_key`")]
    MissingApiKey {
        /// Profile missing credentials.
        name: String,
    },

    /// HTTP transport or protocol failure.
    #[error("HTTP request failed: {0}")]
    Http(#[from] reqwest::Error),

    /// API returned `success: false` or a non-success HTTP status with a body.
    #[error("API error{code}: {message}")]
    Api {
        /// Optional machine-readable error code from the API.
        code: String,
        /// Human-readable error message.
        message: String,
    },
}
