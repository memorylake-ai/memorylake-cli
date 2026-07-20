//! Core library for the MemoryLake CLI.

pub mod api;
pub mod client;
pub mod config;
pub mod credentials;
pub mod error;

pub use client::Client;
pub use config::{
    AuthStatus, DEFAULT_BASE_URL, DEFAULT_PROFILE, ENV_API_KEY, ENV_BASE_URL, FileConfig, Paths,
    ProfileConfig, ResolveOverrides, RuntimeConfig, auth_status, login_api_key, logout,
    mask_api_key, resolve, switch_profile,
};
pub use credentials::{CredentialsFile, LOGIN_METHOD_API_KEY, ProfileCredentials};
pub use error::{Error, Result};
