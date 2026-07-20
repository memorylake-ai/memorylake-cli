//! Local configuration under `~/.memorylake`.

use std::collections::BTreeMap;
use std::fs;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

use crate::credentials::{CredentialsFile, LOGIN_METHOD_API_KEY, ProfileCredentials};
use crate::error::{Error, Result};

/// Default MemoryLake OpenAPI base URL.
pub const DEFAULT_BASE_URL: &str = "https://app.memorylake.ai/openapi/memorylake";

/// Default profile name when none is specified.
pub const DEFAULT_PROFILE: &str = "default";

/// Environment variable that overrides the resolved API key for a process.
pub const ENV_API_KEY: &str = "MEMORYLAKE_API_KEY";

/// Environment variable that overrides the resolved base URL for a process.
pub const ENV_BASE_URL: &str = "MEMORYLAKE_BASE_URL";

/// Directory and file paths for MemoryLake CLI state.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Paths {
    /// Root directory, typically `~/.memorylake`.
    pub root: PathBuf,
    /// Non-secret settings file.
    pub config: PathBuf,
    /// Secret credentials file.
    pub credentials: PathBuf,
}

impl Paths {
    /// Resolve default paths under the user home directory.
    pub fn default_home() -> Result<Self> {
        let home = dirs::home_dir().ok_or(Error::HomeDir)?;
        Ok(Self::from_root(home.join(".memorylake")))
    }

    /// Build paths under an arbitrary root (useful for tests).
    pub fn from_root(root: impl Into<PathBuf>) -> Self {
        let root = root.into();
        Self {
            config: root.join("config.toml"),
            credentials: root.join("credentials.toml"),
            root,
        }
    }
}

/// On-disk non-secret configuration.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct FileConfig {
    /// Currently selected profile name.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub active_profile: Option<String>,
    /// Named profile settings.
    #[serde(default)]
    pub profiles: BTreeMap<String, ProfileConfig>,
}

/// Per-profile non-secret settings.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct ProfileConfig {
    /// Optional base URL override for this profile.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub base_url: Option<String>,
}

/// Resolved settings used to talk to the API.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RuntimeConfig {
    /// Profile name these settings were resolved for.
    pub profile: String,
    /// Fully resolved API base URL.
    pub base_url: String,
    /// Fully resolved API key (never log this).
    pub api_key: String,
}

/// Optional CLI / caller overrides applied during resolution.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct ResolveOverrides {
    /// Profile to use instead of `active_profile`.
    pub profile: Option<String>,
    /// Base URL from a CLI flag.
    pub base_url: Option<String>,
}

/// Auth status information for display (API key is masked).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AuthStatus {
    /// Active profile name, if any.
    pub active_profile: Option<String>,
    /// Resolved base URL for the active profile (or default).
    pub base_url: String,
    /// Masked API key, if credentials exist.
    pub api_key_masked: Option<String>,
    /// Login method stored with the credentials.
    pub login_method: Option<String>,
    /// Whether an API key is available (profile or env).
    pub logged_in: bool,
}

/// Load `config.toml`, returning defaults when the file is missing.
pub fn load_file_config(paths: &Paths) -> Result<FileConfig> {
    read_toml_or_default(&paths.config)
}

/// Persist `config.toml`, creating the parent directory if needed.
pub fn save_file_config(paths: &Paths, config: &FileConfig) -> Result<()> {
    ensure_dir(&paths.root)?;
    write_toml(&paths.config, config)
}

/// Load `credentials.toml`, returning defaults when the file is missing.
pub fn load_credentials(paths: &Paths) -> Result<CredentialsFile> {
    read_toml_or_default(&paths.credentials)
}

/// Persist `credentials.toml` with restrictive permissions on Unix.
pub fn save_credentials(paths: &Paths, credentials: &CredentialsFile) -> Result<()> {
    ensure_dir(&paths.root)?;
    write_toml(&paths.credentials, credentials)?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let perms = fs::Permissions::from_mode(0o600);
        fs::set_permissions(&paths.credentials, perms).map_err(|source| Error::Io {
            action: "chmod",
            path: paths.credentials.clone(),
            source,
        })?;
    }
    Ok(())
}

/// Resolve runtime API settings from files, environment, and overrides.
///
/// Precedence for base URL (highest wins): CLI override → `MEMORYLAKE_BASE_URL` →
/// profile `base_url` → built-in default.
///
/// Precedence for API key: `MEMORYLAKE_API_KEY` → profile credentials.
pub fn resolve(paths: &Paths, overrides: &ResolveOverrides) -> Result<RuntimeConfig> {
    let config = load_file_config(paths)?;
    let credentials = load_credentials(paths)?;

    let profile = overrides
        .profile
        .clone()
        .or_else(|| config.active_profile.clone())
        .ok_or(Error::NotLoggedIn)?;

    if !config.profiles.contains_key(&profile)
        && !credentials.profiles.contains_key(&profile)
        && overrides.profile.is_some()
    {
        return Err(Error::UnknownProfile {
            name: profile.clone(),
        });
    }

    let profile_cfg = config.profiles.get(&profile).cloned().unwrap_or_default();

    let base_url = overrides
        .base_url
        .clone()
        .or_else(|| std::env::var(ENV_BASE_URL).ok().filter(|s| !s.is_empty()))
        .or(profile_cfg.base_url)
        .unwrap_or_else(|| DEFAULT_BASE_URL.to_string());

    let api_key = std::env::var(ENV_API_KEY)
        .ok()
        .filter(|s| !s.is_empty())
        .or_else(|| {
            credentials
                .profiles
                .get(&profile)
                .map(|c| c.api_key.clone())
        })
        .ok_or_else(|| Error::MissingApiKey {
            name: profile.clone(),
        })?;

    Ok(RuntimeConfig {
        profile,
        base_url,
        api_key,
    })
}

/// Store an API-key login for `profile`, optionally set `base_url`, and activate it.
pub fn login_api_key(
    paths: &Paths,
    profile: &str,
    api_key: &str,
    base_url: Option<&str>,
) -> Result<()> {
    let mut config = load_file_config(paths)?;
    let mut credentials = load_credentials(paths)?;

    let entry = config.profiles.entry(profile.to_string()).or_default();
    if let Some(url) = base_url {
        entry.base_url = Some(url.to_string());
    } else if entry.base_url.is_none() {
        entry.base_url = Some(DEFAULT_BASE_URL.to_string());
    }

    credentials.profiles.insert(
        profile.to_string(),
        ProfileCredentials {
            api_key: api_key.to_string(),
            login_method: LOGIN_METHOD_API_KEY.to_string(),
        },
    );
    config.active_profile = Some(profile.to_string());

    save_file_config(paths, &config)?;
    save_credentials(paths, &credentials)?;
    Ok(())
}

/// Remove credentials (and profile config) for `profile`.
pub fn logout(paths: &Paths, profile: Option<&str>) -> Result<String> {
    let mut config = load_file_config(paths)?;
    let mut credentials = load_credentials(paths)?;

    let name = profile
        .map(str::to_string)
        .or_else(|| config.active_profile.clone())
        .ok_or(Error::NotLoggedIn)?;

    if !config.profiles.contains_key(&name) && !credentials.profiles.contains_key(&name) {
        return Err(Error::UnknownProfile { name });
    }

    config.profiles.remove(&name);
    credentials.profiles.remove(&name);
    if config.active_profile.as_deref() == Some(name.as_str()) {
        config.active_profile = None;
    }

    save_file_config(paths, &config)?;
    save_credentials(paths, &credentials)?;
    Ok(name)
}

/// Switch the active profile to `profile`.
pub fn switch_profile(paths: &Paths, profile: &str) -> Result<()> {
    let mut config = load_file_config(paths)?;
    let credentials = load_credentials(paths)?;

    if !config.profiles.contains_key(profile) && !credentials.profiles.contains_key(profile) {
        return Err(Error::UnknownProfile {
            name: profile.to_string(),
        });
    }

    config.active_profile = Some(profile.to_string());
    save_file_config(paths, &config)?;
    Ok(())
}

/// Build a display-oriented auth status snapshot.
pub fn auth_status(paths: &Paths) -> Result<AuthStatus> {
    let config = load_file_config(paths)?;
    let credentials = load_credentials(paths)?;

    let active_profile = config.active_profile.clone();
    let (api_key_masked, login_method, profile_base) = match active_profile.as_deref() {
        Some(name) => {
            let cred = credentials.profiles.get(name);
            let cfg = config.profiles.get(name);
            (
                cred.map(|c| mask_api_key(&c.api_key)),
                cred.map(|c| c.login_method.clone()),
                cfg.and_then(|c| c.base_url.clone()),
            )
        }
        None => (None, None, None),
    };

    let env_key = std::env::var(ENV_API_KEY).ok().filter(|s| !s.is_empty());
    let logged_in = api_key_masked.is_some() || env_key.is_some();

    let base_url = std::env::var(ENV_BASE_URL)
        .ok()
        .filter(|s| !s.is_empty())
        .or(profile_base)
        .unwrap_or_else(|| DEFAULT_BASE_URL.to_string());

    let api_key_masked = api_key_masked.or_else(|| env_key.as_deref().map(mask_api_key));

    Ok(AuthStatus {
        active_profile,
        base_url,
        api_key_masked,
        login_method: login_method.or_else(|| env_key.map(|_| LOGIN_METHOD_API_KEY.to_string())),
        logged_in,
    })
}

/// Mask an API key for safe display (`sk_****abcd`).
pub fn mask_api_key(api_key: &str) -> String {
    let trimmed = api_key.trim();
    if trimmed.len() <= 4 {
        return "****".to_string();
    }
    let suffix = &trimmed[trimmed.len() - 4..];
    if let Some(prefix_end) = trimmed.find('_') {
        let prefix = &trimmed[..=prefix_end];
        format!("{prefix}****{suffix}")
    } else {
        format!("****{suffix}")
    }
}

fn ensure_dir(path: &Path) -> Result<()> {
    fs::create_dir_all(path).map_err(|source| Error::Io {
        action: "create",
        path: path.to_path_buf(),
        source,
    })
}

fn read_toml_or_default<T>(path: &Path) -> Result<T>
where
    T: for<'de> Deserialize<'de> + Default,
{
    if !path.exists() {
        return Ok(T::default());
    }
    let text = fs::read_to_string(path).map_err(|source| Error::Io {
        action: "read",
        path: path.to_path_buf(),
        source,
    })?;
    toml::from_str(&text).map_err(|source| Error::Toml {
        path: path.to_path_buf(),
        source: Box::new(source),
    })
}

fn write_toml<T>(path: &Path, value: &T) -> Result<()>
where
    T: Serialize,
{
    let text = toml::to_string_pretty(value).map_err(|source| Error::Toml {
        path: path.to_path_buf(),
        source: Box::new(source),
    })?;
    fs::write(path, text).map_err(|source| Error::Io {
        action: "write",
        path: path.to_path_buf(),
        source,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::{Mutex, OnceLock};

    fn env_lock() -> &'static Mutex<()> {
        static LOCK: OnceLock<Mutex<()>> = OnceLock::new();
        LOCK.get_or_init(|| Mutex::new(()))
    }

    fn temp_paths() -> Paths {
        let root = std::env::temp_dir().join(format!(
            "memorylake-cli-test-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .expect("time")
                .as_nanos()
        ));
        Paths::from_root(root)
    }

    #[test]
    fn login_switch_logout_round_trip() {
        let paths = temp_paths();
        login_api_key(&paths, "default", "sk_testkey1234", None).unwrap();
        login_api_key(
            &paths,
            "dev",
            "sk_devkey5678",
            Some("https://example.test/openapi/memorylake"),
        )
        .unwrap();

        let status = auth_status(&paths).unwrap();
        assert_eq!(status.active_profile.as_deref(), Some("dev"));
        assert!(status.logged_in);
        assert_eq!(status.login_method.as_deref(), Some("api_key"));
        assert_eq!(status.base_url, "https://example.test/openapi/memorylake");

        switch_profile(&paths, "default").unwrap();
        let runtime = resolve(&paths, &ResolveOverrides::default()).unwrap();
        assert_eq!(runtime.profile, "default");
        assert_eq!(runtime.api_key, "sk_testkey1234");
        assert_eq!(runtime.base_url, DEFAULT_BASE_URL);

        let removed = logout(&paths, Some("default")).unwrap();
        assert_eq!(removed, "default");
        assert!(matches!(
            resolve(&paths, &ResolveOverrides::default()),
            Err(Error::NotLoggedIn)
        ));

        let _ = fs::remove_dir_all(&paths.root);
    }

    #[test]
    fn resolution_precedence() {
        let _guard = env_lock().lock().unwrap();
        let paths = temp_paths();
        login_api_key(
            &paths,
            "default",
            "sk_profilekey000",
            Some("https://profile.example/openapi/memorylake"),
        )
        .unwrap();

        // SAFETY: guarded by env_lock; restored before unlock.
        unsafe {
            std::env::remove_var(ENV_API_KEY);
            std::env::remove_var(ENV_BASE_URL);
        }

        let runtime = resolve(
            &paths,
            &ResolveOverrides {
                profile: None,
                base_url: Some("https://cli.example/openapi/memorylake".into()),
            },
        )
        .unwrap();
        assert_eq!(runtime.base_url, "https://cli.example/openapi/memorylake");
        assert_eq!(runtime.api_key, "sk_profilekey000");

        // SAFETY: guarded by env_lock; restored before unlock.
        unsafe {
            std::env::set_var(ENV_BASE_URL, "https://env.example/openapi/memorylake");
            std::env::set_var(ENV_API_KEY, "sk_envkey9999");
        }

        let runtime = resolve(&paths, &ResolveOverrides::default()).unwrap();
        assert_eq!(runtime.base_url, "https://env.example/openapi/memorylake");
        assert_eq!(runtime.api_key, "sk_envkey9999");

        let runtime = resolve(
            &paths,
            &ResolveOverrides {
                profile: None,
                base_url: Some("https://cli.example/openapi/memorylake".into()),
            },
        )
        .unwrap();
        assert_eq!(runtime.base_url, "https://cli.example/openapi/memorylake");

        // SAFETY: guarded by env_lock.
        unsafe {
            std::env::remove_var(ENV_API_KEY);
            std::env::remove_var(ENV_BASE_URL);
        }
        let _ = fs::remove_dir_all(&paths.root);
    }

    #[test]
    fn mask_api_key_keeps_prefix_and_suffix() {
        assert_eq!(mask_api_key("sk_abcdefghij"), "sk_****ghij");
        assert_eq!(mask_api_key("ab"), "****");
    }
}
