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
    ///
    /// A browser login stores `api_key` here too, because what it leaves
    /// behind *is* an API key and every request uses it as one. Several
    /// `memorylake` binaries of different ages can share this file (one per
    /// agent integration), and an older one refuses any method it does not
    /// know; [`Self::issued`] is what tells a browser login apart.
    pub login_method: String,
    /// Set when [`Self::api_key`] was created by a browser login rather than
    /// pasted in.
    ///
    /// Absent for a pasted key, so the file a key login writes is unchanged.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub issued: Option<IssuedKey>,
}

/// What the server reported about an API key it issued to a browser login.
///
/// Kept beside the key because it describes that key and nothing else: it is
/// what `auth status` shows, and its presence is what makes `auth logout`
/// revoke the key instead of only forgetting it.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct IssuedKey {
    /// The key's id in the console's API key list.
    pub id: String,
    /// The name the key was created under.
    pub name: String,
    /// When the key stops working, in Unix seconds, if the server set a limit.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub expires_at: Option<i64>,
    /// The team the user picked on the consent page.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub tenant_id: Option<String>,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_pasted_key_is_written_exactly_as_before() {
        // Older binaries share this file. They ignore fields they do not know
        // when reading, but the shape a key login writes must not change at
        // all, so nothing about a pasted key depends on that.
        let mut file = CredentialsFile::default();
        file.profiles.insert(
            "default".into(),
            ProfileCredentials {
                api_key: "sk_pasted".into(),
                login_method: LOGIN_METHOD_API_KEY.into(),
                issued: None,
            },
        );

        assert_eq!(
            toml::to_string_pretty(&file).expect("serialize"),
            "[profiles.default]\napi_key = \"sk_pasted\"\nlogin_method = \"api_key\"\n"
        );
    }

    #[test]
    fn an_issued_key_round_trips_with_what_the_server_said_about_it() {
        let mut file = CredentialsFile::default();
        file.profiles.insert(
            "default".into(),
            ProfileCredentials {
                api_key: "sk_issued".into(),
                login_method: LOGIN_METHOD_API_KEY.into(),
                issued: Some(IssuedKey {
                    id: "123".into(),
                    name: "memorylake CLI · my-mac".into(),
                    expires_at: Some(1_786_953_600),
                    tenant_id: Some("tenant-1".into()),
                }),
            },
        );

        let text = toml::to_string_pretty(&file).expect("serialize");
        assert!(
            text.contains("login_method = \"api_key\""),
            "an older binary must still be able to use the key: {text}"
        );
        let read: CredentialsFile = toml::from_str(&text).expect("parse");
        assert_eq!(read, file);
    }

    #[test]
    fn credentials_written_before_issued_keys_existed_still_load() {
        let read: CredentialsFile = toml::from_str(
            "[profiles.default]\napi_key = \"sk_old\"\nlogin_method = \"api_key\"\n",
        )
        .expect("parse");
        assert_eq!(read.profiles["default"].issued, None);
    }
}
