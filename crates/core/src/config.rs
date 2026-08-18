//! Local configuration under `~/.memorylake`.

use std::collections::BTreeMap;
use std::fmt;
use std::fs;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

use crate::credentials::{CredentialsFile, LOGIN_METHOD_API_KEY, ProfileCredentials};
use crate::error::{Error, Result};

/// Default MemoryLake OpenAPI base URL (global deployment).
pub const DEFAULT_BASE_URL: &str = "https://app.memorylake.ai/openapi/memorylake";

/// Base URL for the China deployment.
///
/// A separate deployment rather than a mirror, so accounts and data do not
/// carry over between it and [`DEFAULT_BASE_URL`]. Offered during interactive
/// login because picking the wrong one fails in a way that reads like a bad API
/// key rather than a wrong endpoint.
pub const CN_BASE_URL: &str = "https://app.memorylake.cn/openapi/memorylake";

/// Default profile name when none is specified.
pub const DEFAULT_PROFILE: &str = "default";

/// Environment variable used as API-key fallback when the profile has none.
pub const ENV_API_KEY: &str = "MEMORYLAKE_API_KEY";

/// Environment variable used as base-URL fallback when the profile has none.
pub const ENV_BASE_URL: &str = "MEMORYLAKE_BASE_URL";

/// Environment variable holding a fallback workspace id.
pub const ENV_WORKSPACE: &str = "MEMORYLAKE_WORKSPACE";

/// Environment variable overriding where CLI state is stored.
///
/// Points at the directory holding `config.toml` and `credentials.toml`,
/// replacing `~/.memorylake` entirely.
pub const ENV_CONFIG_DIR: &str = "MEMORYLAKE_CONFIG_DIR";

/// Where the resolved API key came from.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ApiKeySource {
    /// `credentials.toml` for the selected profile.
    Profile,
    /// `MEMORYLAKE_API_KEY` fallback.
    Env,
}

impl fmt::Display for ApiKeySource {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Profile => write!(f, "profile"),
            Self::Env => write!(f, "env"),
        }
    }
}

/// Where the resolved workspace came from.
///
/// Has no `Default` variant, unlike [`BaseUrlSource`]: there is no sensible
/// built-in workspace, so "nowhere" is represented by resolving to `None`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum WorkspaceSource {
    /// CLI `--workspace` for this invocation.
    Cli,
    /// Remembered `workspace` in `config.toml` for the profile.
    Profile,
    /// `MEMORYLAKE_WORKSPACE` fallback.
    Env,
}

impl fmt::Display for WorkspaceSource {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Cli => write!(f, "cli"),
            Self::Profile => write!(f, "profile"),
            Self::Env => write!(f, "env"),
        }
    }
}

/// Where the resolved base URL came from.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BaseUrlSource {
    /// CLI `--base-url` for this invocation.
    Cli,
    /// Explicit `base_url` in `config.toml` for the profile.
    Profile,
    /// `MEMORYLAKE_BASE_URL` fallback.
    Env,
    /// Built-in default.
    Default,
}

impl fmt::Display for BaseUrlSource {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Cli => write!(f, "cli"),
            Self::Profile => write!(f, "profile"),
            Self::Env => write!(f, "env"),
            Self::Default => write!(f, "default"),
        }
    }
}

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
    /// Resolve where CLI state lives: `MEMORYLAKE_CONFIG_DIR`, else
    /// `~/.memorylake`.
    ///
    /// The override exists because the home directory is not always
    /// redirectable. On Windows `dirs::home_dir()` calls
    /// `SHGetKnownFolderPath(FOLDERID_Profile)`, which ignores `USERPROFILE`
    /// and `HOME` entirely — so pointing a test, a container, or a CI job at a
    /// scratch directory by setting those variables silently has no effect and
    /// the real user's config is read instead.
    pub fn default_home() -> Result<Self> {
        if let Some(dir) = std::env::var_os(ENV_CONFIG_DIR) {
            let dir = PathBuf::from(dir);
            if !dir.as_os_str().is_empty() {
                return Ok(Self::from_root(dir));
            }
        }
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
    /// Workspace to use when a command is not given one explicitly.
    ///
    /// Set by `workspace use`. Absent means every command must name its own
    /// workspace, which is how the CLI behaved before this existed.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub workspace: Option<String>,
}

/// Resolved settings used to talk to the API.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RuntimeConfig {
    /// Profile name these settings were resolved for.
    pub profile: String,
    /// Fully resolved API base URL.
    pub base_url: String,
    /// Where [`Self::base_url`] came from.
    pub base_url_source: BaseUrlSource,
    /// Fully resolved API key (never log this).
    pub api_key: String,
    /// Where [`Self::api_key`] came from.
    pub api_key_source: ApiKeySource,
    /// Login method for the selected profile (`api_key`, `oauth`, …).
    pub login_method: String,
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
    /// Where [`Self::base_url`] came from.
    pub base_url_source: BaseUrlSource,
    /// Masked API key, if one was resolved.
    pub api_key_masked: Option<String>,
    /// Where the API key came from, when present.
    pub api_key_source: Option<ApiKeySource>,
    /// Login method stored with the profile credentials.
    pub login_method: Option<String>,
    /// Whether credentials can be resolved for the active profile.
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
/// Profile selection: CLI `--profile` → `active_profile` → not logged in.
///
/// Base URL (highest wins): CLI `--base-url` → explicit profile `base_url` in
/// `config.toml` → `MEMORYLAKE_BASE_URL` → built-in default.
///
/// API key (when profile `login_method` is `api_key`): non-empty key in
/// `credentials.toml` → `MEMORYLAKE_API_KEY` → missing key error.
///
/// Environment variables alone do not create a session; a profile must be selected.
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
    let (base_url, base_url_source) = resolve_base_url(
        overrides.base_url.as_deref(),
        profile_cfg.base_url.as_deref(),
    );

    let cred = credentials
        .profiles
        .get(&profile)
        .ok_or_else(|| Error::MissingApiKey {
            name: profile.clone(),
        })?;

    if cred.login_method != LOGIN_METHOD_API_KEY {
        return Err(Error::UnsupportedLoginMethod {
            name: profile,
            method: cred.login_method.clone(),
        });
    }

    let (api_key, api_key_source) = resolve_api_key_api_method(&profile, &cred.api_key)?;

    Ok(RuntimeConfig {
        profile,
        base_url,
        base_url_source,
        api_key,
        api_key_source,
        login_method: cred.login_method.clone(),
    })
}

fn resolve_base_url(
    cli_base_url: Option<&str>,
    profile_base_url: Option<&str>,
) -> (String, BaseUrlSource) {
    if let Some(url) = cli_base_url.map(str::trim).filter(|s| !s.is_empty()) {
        return (url.to_string(), BaseUrlSource::Cli);
    }
    if let Some(url) = profile_base_url.map(str::trim).filter(|s| !s.is_empty()) {
        return (url.to_string(), BaseUrlSource::Profile);
    }
    if let Ok(url) = std::env::var(ENV_BASE_URL) {
        let url = url.trim();
        if !url.is_empty() {
            return (url.to_string(), BaseUrlSource::Env);
        }
    }
    (DEFAULT_BASE_URL.to_string(), BaseUrlSource::Default)
}

/// Resolve the base URL for `profile` using the same precedence as [`resolve`]:
/// CLI override → profile `config.toml` → `MEMORYLAKE_BASE_URL` → default.
///
/// Does not require credentials; used by login probing before credentials exist.
pub fn resolve_profile_base_url(
    paths: &Paths,
    profile: &str,
    cli_base_url: Option<&str>,
) -> Result<(String, BaseUrlSource)> {
    let config = load_file_config(paths)?;
    let profile_base = config
        .profiles
        .get(profile)
        .and_then(|c| c.base_url.as_deref());
    Ok(resolve_base_url(cli_base_url, profile_base))
}

/// Remember `workspace` as the default for `profile`.
///
/// Creates the profile entry when it does not exist yet, so this works
/// immediately after a login that wrote only credentials.
pub fn set_profile_workspace(paths: &Paths, profile: &str, workspace: &str) -> Result<()> {
    let mut config = load_file_config(paths)?;
    config
        .profiles
        .entry(profile.to_string())
        .or_default()
        .workspace = Some(workspace.to_string());
    save_file_config(paths, &config)?;
    Ok(())
}

/// Forget the default workspace for `profile`.
///
/// Absent profile or absent workspace are both no-ops: clearing something that
/// is already clear is not an error.
pub fn clear_profile_workspace(paths: &Paths, profile: &str) -> Result<()> {
    let mut config = load_file_config(paths)?;
    if let Some(entry) = config.profiles.get_mut(profile) {
        entry.workspace = None;
        save_file_config(paths, &config)?;
    }
    Ok(())
}

/// Resolve the workspace for `profile`: CLI flag → profile config → env.
///
/// Mirrors the base-URL precedence, where the profile also outranks the
/// environment: a value written by `workspace use` is an explicit choice this
/// user made, while the environment is often inherited from a shell they did
/// not set up. `None` means no workspace is known and the caller must say so.
pub fn resolve_profile_workspace(
    paths: &Paths,
    profile: &str,
    cli_workspace: Option<&str>,
) -> Result<Option<(String, WorkspaceSource)>> {
    if let Some(workspace) = non_empty(cli_workspace) {
        return Ok(Some((workspace, WorkspaceSource::Cli)));
    }

    let config = load_file_config(paths)?;
    if let Some(workspace) = config
        .profiles
        .get(profile)
        .and_then(|c| c.workspace.as_deref())
        .and_then(|value| non_empty(Some(value)))
    {
        return Ok(Some((workspace, WorkspaceSource::Profile)));
    }

    if let Ok(env_workspace) = std::env::var(ENV_WORKSPACE)
        && let Some(workspace) = non_empty(Some(&env_workspace))
    {
        return Ok(Some((workspace, WorkspaceSource::Env)));
    }

    Ok(None)
}

/// Trim a candidate and discard it when nothing is left.
///
/// A blank value is treated as absent everywhere, so `--workspace ""` or an
/// empty env var falls through to the next source rather than being sent.
fn non_empty(value: Option<&str>) -> Option<String> {
    value
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(str::to_string)
}

fn resolve_api_key_api_method(
    profile: &str,
    profile_api_key: &str,
) -> Result<(String, ApiKeySource)> {
    let profile_key = profile_api_key.trim();
    if !profile_key.is_empty() {
        return Ok((profile_key.to_string(), ApiKeySource::Profile));
    }
    if let Ok(env_key) = std::env::var(ENV_API_KEY) {
        let env_key = env_key.trim();
        if !env_key.is_empty() {
            return Ok((env_key.to_string(), ApiKeySource::Env));
        }
    }
    Err(Error::MissingApiKey {
        name: profile.to_string(),
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
///
/// Env-only credentials without an active profile do not count as logged in.
pub fn auth_status(paths: &Paths) -> Result<AuthStatus> {
    let config = load_file_config(paths)?;
    let credentials = load_credentials(paths)?;
    let active_profile = config.active_profile.clone();

    let Some(profile) = active_profile.clone() else {
        return Ok(AuthStatus {
            active_profile: None,
            base_url: DEFAULT_BASE_URL.to_string(),
            base_url_source: BaseUrlSource::Default,
            api_key_masked: None,
            api_key_source: None,
            login_method: None,
            logged_in: false,
        });
    };

    let profile_cfg = config.profiles.get(&profile).cloned().unwrap_or_default();
    let (base_url, base_url_source) = resolve_base_url(None, profile_cfg.base_url.as_deref());
    let login_method = credentials
        .profiles
        .get(&profile)
        .map(|c| c.login_method.clone());

    match resolve(paths, &ResolveOverrides::default()) {
        Ok(runtime) => Ok(AuthStatus {
            active_profile: Some(profile),
            base_url: runtime.base_url,
            base_url_source: runtime.base_url_source,
            api_key_masked: Some(mask_api_key(&runtime.api_key)),
            api_key_source: Some(runtime.api_key_source),
            login_method: Some(runtime.login_method),
            logged_in: true,
        }),
        Err(Error::MissingApiKey { .. } | Error::UnsupportedLoginMethod { .. }) => Ok(AuthStatus {
            active_profile: Some(profile),
            base_url,
            base_url_source,
            api_key_masked: None,
            api_key_source: None,
            login_method,
            logged_in: false,
        }),
        Err(err) => Err(err),
    }
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
    fn the_config_dir_can_be_redirected_by_environment() {
        // The only way to sandbox CLI state on Windows: `dirs::home_dir()` there
        // ignores USERPROFILE and HOME, so redirecting the home directory does
        // nothing and the real user's config would be read instead.
        let _guard = env_lock().lock().unwrap();
        let scratch = temp_paths();
        unsafe {
            std::env::set_var(ENV_CONFIG_DIR, &scratch.root);
        }
        let resolved = Paths::default_home();
        unsafe {
            std::env::remove_var(ENV_CONFIG_DIR);
        }
        let resolved = resolved.expect("resolve paths");

        assert_eq!(resolved.root, scratch.root);
        assert_eq!(resolved.config, scratch.root.join("config.toml"));
        assert_eq!(
            resolved.credentials,
            scratch.root.join("credentials.toml"),
            "both files live directly under the override, with no .memorylake suffix added"
        );
    }

    #[test]
    fn a_blank_config_dir_falls_back_to_the_home_directory() {
        // An empty variable is a common artefact of shell quoting; treating it
        // as a path would put state in the process's working directory.
        let _guard = env_lock().lock().unwrap();
        unsafe {
            std::env::set_var(ENV_CONFIG_DIR, "");
        }
        let resolved = Paths::default_home();
        unsafe {
            std::env::remove_var(ENV_CONFIG_DIR);
        }

        let resolved = resolved.expect("resolve paths");
        assert!(
            resolved.root.ends_with(".memorylake"),
            "expected the home-directory default, got {:?}",
            resolved.root
        );
    }

    #[test]
    fn a_workspace_round_trips_through_the_profile() {
        let _guard = env_lock().lock().unwrap();
        let paths = temp_paths();
        login_api_key(&paths, "default", "sk_testkey1234", None).unwrap();

        assert_eq!(
            resolve_profile_workspace(&paths, "default", None).unwrap(),
            None,
            "nothing is remembered until `workspace use` runs"
        );

        set_profile_workspace(&paths, "default", "ws-1234").unwrap();
        assert_eq!(
            resolve_profile_workspace(&paths, "default", None).unwrap(),
            Some(("ws-1234".to_string(), WorkspaceSource::Profile))
        );

        clear_profile_workspace(&paths, "default").unwrap();
        assert_eq!(
            resolve_profile_workspace(&paths, "default", None).unwrap(),
            None
        );
    }

    #[test]
    fn a_workspace_can_be_remembered_before_the_profile_has_config() {
        // `workspace use` may run right after a login that wrote credentials
        // only, so the profile entry has to be created on demand.
        let _guard = env_lock().lock().unwrap();
        let paths = temp_paths();
        set_profile_workspace(&paths, "fresh", "ws-new").unwrap();
        assert_eq!(
            resolve_profile_workspace(&paths, "fresh", None).unwrap(),
            Some(("ws-new".to_string(), WorkspaceSource::Profile))
        );
    }

    #[test]
    fn the_cli_flag_outranks_the_remembered_workspace() {
        let _guard = env_lock().lock().unwrap();
        let paths = temp_paths();
        set_profile_workspace(&paths, "default", "ws-remembered").unwrap();
        assert_eq!(
            resolve_profile_workspace(&paths, "default", Some("ws-explicit")).unwrap(),
            Some(("ws-explicit".to_string(), WorkspaceSource::Cli))
        );
    }

    #[test]
    fn the_profile_outranks_the_environment() {
        // Same precedence as the base URL: a value written by `workspace use`
        // is a choice this user made, while the environment is often inherited.
        let _guard = env_lock().lock().unwrap();
        let paths = temp_paths();
        set_profile_workspace(&paths, "default", "ws-remembered").unwrap();
        unsafe {
            std::env::set_var(ENV_WORKSPACE, "ws-from-env");
        }
        let resolved = resolve_profile_workspace(&paths, "default", None).unwrap();
        unsafe {
            std::env::remove_var(ENV_WORKSPACE);
        }
        assert_eq!(
            resolved,
            Some(("ws-remembered".to_string(), WorkspaceSource::Profile))
        );
    }

    #[test]
    fn the_environment_is_used_when_nothing_is_remembered() {
        let _guard = env_lock().lock().unwrap();
        let paths = temp_paths();
        unsafe {
            std::env::set_var(ENV_WORKSPACE, "ws-from-env");
        }
        let resolved = resolve_profile_workspace(&paths, "default", None).unwrap();
        unsafe {
            std::env::remove_var(ENV_WORKSPACE);
        }
        assert_eq!(
            resolved,
            Some(("ws-from-env".to_string(), WorkspaceSource::Env))
        );
    }

    #[test]
    fn blank_workspaces_fall_through_instead_of_being_sent() {
        // `--workspace ""` or an empty env var must not become a request for a
        // workspace named the empty string.
        let _guard = env_lock().lock().unwrap();
        let paths = temp_paths();
        set_profile_workspace(&paths, "default", "ws-remembered").unwrap();
        assert_eq!(
            resolve_profile_workspace(&paths, "default", Some("   ")).unwrap(),
            Some(("ws-remembered".to_string(), WorkspaceSource::Profile)),
            "a blank flag falls through to the profile"
        );

        clear_profile_workspace(&paths, "default").unwrap();
        unsafe {
            std::env::set_var(ENV_WORKSPACE, "  ");
        }
        let resolved = resolve_profile_workspace(&paths, "default", None).unwrap();
        unsafe {
            std::env::remove_var(ENV_WORKSPACE);
        }
        assert_eq!(resolved, None, "a blank env var is not a workspace");
    }

    #[test]
    fn a_remembered_workspace_survives_alongside_a_base_url() {
        let _guard = env_lock().lock().unwrap();
        let paths = temp_paths();
        login_api_key(
            &paths,
            "default",
            "sk_testkey1234",
            Some("https://example.test/openapi/memorylake"),
        )
        .unwrap();
        set_profile_workspace(&paths, "default", "ws-1234").unwrap();

        let config = load_file_config(&paths).unwrap();
        let profile = config.profiles.get("default").expect("profile");
        assert_eq!(profile.workspace.as_deref(), Some("ws-1234"));
        assert_eq!(
            profile.base_url.as_deref(),
            Some("https://example.test/openapi/memorylake"),
            "remembering a workspace must not drop the profile's base URL"
        );
    }

    #[test]
    fn clearing_an_unknown_profile_is_not_an_error() {
        let _guard = env_lock().lock().unwrap();
        let paths = temp_paths();
        clear_profile_workspace(&paths, "never-existed").unwrap();
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
        assert_eq!(status.api_key_source, Some(ApiKeySource::Profile));
        assert_eq!(status.base_url_source, BaseUrlSource::Profile);
        assert_eq!(status.base_url, "https://example.test/openapi/memorylake");

        switch_profile(&paths, "default").unwrap();
        let runtime = resolve(&paths, &ResolveOverrides::default()).unwrap();
        assert_eq!(runtime.profile, "default");
        assert_eq!(runtime.api_key, "sk_testkey1234");
        assert_eq!(runtime.api_key_source, ApiKeySource::Profile);
        assert_eq!(runtime.base_url, DEFAULT_BASE_URL);
        assert_eq!(runtime.base_url_source, BaseUrlSource::Profile);

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
        assert_eq!(runtime.base_url_source, BaseUrlSource::Cli);
        assert_eq!(runtime.api_key, "sk_profilekey000");
        assert_eq!(runtime.api_key_source, ApiKeySource::Profile);

        // SAFETY: guarded by env_lock; restored before unlock.
        unsafe {
            std::env::set_var(ENV_BASE_URL, "https://env.example/openapi/memorylake");
            std::env::set_var(ENV_API_KEY, "sk_envkey9999");
        }

        // Profile key and profile base_url win over env.
        let runtime = resolve(&paths, &ResolveOverrides::default()).unwrap();
        assert_eq!(
            runtime.base_url,
            "https://profile.example/openapi/memorylake"
        );
        assert_eq!(runtime.base_url_source, BaseUrlSource::Profile);
        assert_eq!(runtime.api_key, "sk_profilekey000");
        assert_eq!(runtime.api_key_source, ApiKeySource::Profile);

        // Clear profile base_url → env base URL is used.
        let mut config = load_file_config(&paths).unwrap();
        config
            .profiles
            .get_mut("default")
            .expect("default profile")
            .base_url = None;
        save_file_config(&paths, &config).unwrap();

        let runtime = resolve(&paths, &ResolveOverrides::default()).unwrap();
        assert_eq!(runtime.base_url, "https://env.example/openapi/memorylake");
        assert_eq!(runtime.base_url_source, BaseUrlSource::Env);

        // Empty profile api_key → env key fallback.
        let mut credentials = load_credentials(&paths).unwrap();
        credentials
            .profiles
            .get_mut("default")
            .expect("default credentials")
            .api_key
            .clear();
        save_credentials(&paths, &credentials).unwrap();

        let runtime = resolve(&paths, &ResolveOverrides::default()).unwrap();
        assert_eq!(runtime.api_key, "sk_envkey9999");
        assert_eq!(runtime.api_key_source, ApiKeySource::Env);
        assert_eq!(runtime.login_method, LOGIN_METHOD_API_KEY);

        // CLI base URL still wins over profile/env.
        let runtime = resolve(
            &paths,
            &ResolveOverrides {
                profile: None,
                base_url: Some("https://cli.example/openapi/memorylake".into()),
            },
        )
        .unwrap();
        assert_eq!(runtime.base_url, "https://cli.example/openapi/memorylake");
        assert_eq!(runtime.base_url_source, BaseUrlSource::Cli);

        // Env alone without an active profile is not a session.
        let mut config = load_file_config(&paths).unwrap();
        config.active_profile = None;
        save_file_config(&paths, &config).unwrap();
        assert!(matches!(
            resolve(&paths, &ResolveOverrides::default()),
            Err(Error::NotLoggedIn)
        ));
        let status = auth_status(&paths).unwrap();
        assert!(!status.logged_in);
        assert!(status.active_profile.is_none());

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

    #[test]
    fn resolve_profile_base_url_follows_precedence() {
        let _guard = env_lock().lock().unwrap();
        let paths = temp_paths();

        // SAFETY: guarded by env_lock; restored before unlock.
        unsafe {
            std::env::remove_var(ENV_BASE_URL);
        }

        let (url, source) = resolve_profile_base_url(&paths, "default", None).unwrap();
        assert_eq!(url, DEFAULT_BASE_URL);
        assert_eq!(source, BaseUrlSource::Default);

        // SAFETY: guarded by env_lock.
        unsafe {
            std::env::set_var(ENV_BASE_URL, "https://env.example/openapi/memorylake");
        }
        let (url, source) = resolve_profile_base_url(&paths, "default", None).unwrap();
        assert_eq!(url, "https://env.example/openapi/memorylake");
        assert_eq!(source, BaseUrlSource::Env);

        let mut config = FileConfig::default();
        config.profiles.insert(
            "default".to_string(),
            ProfileConfig {
                base_url: Some("https://profile.example/openapi/memorylake".into()),
                ..Default::default()
            },
        );
        save_file_config(&paths, &config).unwrap();

        let (url, source) = resolve_profile_base_url(&paths, "default", None).unwrap();
        assert_eq!(url, "https://profile.example/openapi/memorylake");
        assert_eq!(source, BaseUrlSource::Profile);

        let (url, source) = resolve_profile_base_url(
            &paths,
            "default",
            Some("https://cli.example/openapi/memorylake"),
        )
        .unwrap();
        assert_eq!(url, "https://cli.example/openapi/memorylake");
        assert_eq!(source, BaseUrlSource::Cli);

        // SAFETY: guarded by env_lock.
        unsafe {
            std::env::remove_var(ENV_BASE_URL);
        }
        let _ = fs::remove_dir_all(&paths.root);
    }
}
