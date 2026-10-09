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

    /// Local file has no bytes to upload.
    #[error("cannot upload empty file {path}; the upload API requires at least 1 byte")]
    EmptyUpload {
        /// Path of the empty file.
        path: PathBuf,
    },

    /// The upload session's part plan does not describe the file being sent.
    #[error("upload session for {path} returned an unusable part plan: {reason}")]
    UploadPlan {
        /// Path being uploaded.
        path: PathBuf,
        /// What is wrong with the plan.
        reason: String,
    },

    /// The file changed on disk while its upload was in flight.
    #[error(
        "{path} changed size during upload (session created for {expected} bytes, file is now {actual}); re-run the upload"
    )]
    UploadSizeChanged {
        /// Path being uploaded.
        path: PathBuf,
        /// Size the upload session was created for.
        expected: u64,
        /// Size observed mid-upload.
        actual: u64,
    },

    /// A pre-signed part URL was refused and cannot be retried.
    ///
    /// The signature is fixed for the lifetime of the session, so every further
    /// attempt fails identically; only a fresh session can recover.
    #[error(
        "pre-signed URL for part {number} of {path} was refused (HTTP {status}); upload sessions are short-lived — re-run the upload"
    )]
    UploadUrlRefused {
        /// Path being uploaded.
        path: PathBuf,
        /// 1-based part number.
        number: u32,
        /// Status returned by the storage backend.
        status: u16,
    },

    /// A part exhausted its retry budget.
    #[error("part {number} of {total} for {path} failed after {attempts} attempt(s)")]
    PartUpload {
        /// Path being uploaded.
        path: PathBuf,
        /// 1-based part number.
        number: u32,
        /// Total parts in the plan.
        total: u32,
        /// Attempts made before giving up.
        attempts: u32,
        /// Underlying transport or storage failure.
        #[source]
        source: crate::client::PartUploadError,
    },

    /// A local file cannot be used as a skill package.
    #[error("{path} cannot be used as a skill package: {reason}")]
    InvalidSkillPackage {
        /// Path the caller supplied.
        path: PathBuf,
        /// What is wrong with it.
        reason: String,
    },

    /// A single-shot pre-signed upload was refused by the storage backend.
    ///
    /// The signature is fixed, so resending to the same URL cannot help; the
    /// caller has to request a fresh upload slot.
    #[error(
        "storage refused the upload (HTTP {status}); upload URLs are short-lived — re-run the command\n{body}"
    )]
    StorageUploadRefused {
        /// Status returned by the storage backend.
        status: u16,
        /// Response body, with credential-bearing parameters redacted.
        body: String,
    },

    /// A pre-signed upload never got a response from the storage backend
    /// (connection refused or reset, timeout).
    ///
    /// Kept apart from [`Error::Http`], whose message is about the MemoryLake
    /// API: the failing host here is storage. The source has had its URL
    /// stripped, because that URL is a working credential.
    #[error(
        "could not reach storage to upload the file; check your network and re-run the command"
    )]
    StorageTransport {
        /// Underlying transport error, without its URL.
        #[source]
        source: reqwest::Error,
    },

    /// HTTP transport or protocol failure.
    #[error("{}", format_http_error(.0))]
    Http(#[from] reqwest::Error),

    /// API returned an error or an unexpected HTTP response body.
    #[error("{message}")]
    Api {
        /// Human-readable error message (may include HTTP status and body).
        message: String,
        /// Machine-readable `error_code` from the API envelope, when the
        /// response carried one. Lets callers branch on a specific failure
        /// (`NOT_FOUND`, …) without parsing the display string.
        code: Option<String>,
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
