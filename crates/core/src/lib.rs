//! Core library for the MemoryLake CLI.

pub mod api;
pub mod client;
pub mod config;
pub mod credentials;
pub mod error;

#[cfg(test)]
mod test_support;

pub use client::{Client, PartUploadError};
pub use config::{
    ApiKeySource, AuthStatus, BaseUrlSource, CN_BASE_URL, DEFAULT_BASE_URL, DEFAULT_PROFILE,
    ENV_API_KEY, ENV_BASE_URL, ENV_CONFIG_DIR, ENV_WORKSPACE, FileConfig, Paths, ProfileConfig,
    ResolveOverrides, RuntimeConfig, WorkspaceSource, auth_status, clear_profile_workspace,
    load_file_config, login_api_key, logout, mask_api_key, resolve, resolve_profile_base_url,
    resolve_profile_workspace, set_profile_workspace, switch_profile,
};
pub use credentials::{
    CredentialsFile, LOGIN_METHOD_API_KEY, LOGIN_METHOD_OAUTH, ProfileCredentials,
};
pub use error::{Error, Result};
