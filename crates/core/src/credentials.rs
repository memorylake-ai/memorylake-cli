//! Credential storage under `~/.memorylake/credentials.toml`.

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};

/// Login method identifier for API-key based auth.
pub const LOGIN_METHOD_API_KEY: &str = "api_key";

/// Login method identifier for OAuth-based auth.
pub const LOGIN_METHOD_OAUTH: &str = "oauth";

/// On-disk secret credentials file.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct CredentialsFile {
    /// Named profile credentials.
    #[serde(default)]
    pub profiles: BTreeMap<String, ProfileCredentials>,
}

/// Per-profile secret credentials.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ProfileCredentials {
    /// Bearer API key (`sk_...`).
    pub api_key: String,
    /// How this profile was authenticated (e.g. `api_key`).
    pub login_method: String,
}
