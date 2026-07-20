//! `memorylake auth` commands.

use anyhow::{Context, Result};
use clap::Subcommand;
use memorylake_core::api::workspaces::{ListWorkspacesParams, list_workspaces};
use memorylake_core::{
    Client, DEFAULT_PROFILE, Paths, ResolveOverrides, auth_status, login_api_key, logout, resolve,
    switch_profile,
};

/// Authentication subcommands.
#[derive(Debug, Subcommand)]
pub enum AuthCommand {
    /// Authenticate and store credentials for a profile.
    Login {
        #[command(subcommand)]
        method: LoginMethod,
    },
    /// Remove stored credentials for a profile.
    Logout {
        /// Profile to log out (defaults to the active profile).
        #[arg(long)]
        profile: Option<String>,
    },
    /// Validate credentials by calling the API.
    Refresh {
        /// Profile to refresh (defaults to the active profile).
        #[arg(long)]
        profile: Option<String>,
    },
    /// Show the active profile and masked credentials.
    Status,
    /// Switch the active profile.
    Switch {
        /// Profile name to activate.
        profile: String,
    },
}

/// Supported login methods.
#[derive(Debug, Subcommand)]
pub enum LoginMethod {
    /// Log in with a MemoryLake API key.
    #[command(name = "api_key")]
    ApiKey {
        /// API key (`sk_...`).
        #[arg(long = "api-key")]
        api_key: String,
        /// Profile to write (defaults to `default`).
        #[arg(long)]
        profile: Option<String>,
        /// Base URL to store on the profile.
        #[arg(long)]
        base_url: Option<String>,
    },
}

/// Execute an `auth` subcommand.
pub fn run(
    command: AuthCommand,
    global_profile: Option<String>,
    global_base_url: Option<String>,
) -> Result<()> {
    let paths = Paths::default_home().context("resolve MemoryLake config paths")?;

    match command {
        AuthCommand::Login { method } => match method {
            LoginMethod::ApiKey {
                api_key,
                profile,
                base_url,
            } => {
                let profile = profile
                    .or(global_profile)
                    .unwrap_or_else(|| DEFAULT_PROFILE.to_string());
                let base_url = base_url.or(global_base_url);
                login_api_key(&paths, &profile, &api_key, base_url.as_deref())
                    .with_context(|| format!("login api_key for profile `{profile}`"))?;
                println!("Logged in to profile `{profile}` using api_key");
            }
        },
        AuthCommand::Logout { profile } => {
            let profile = profile.or(global_profile);
            let name = logout(&paths, profile.as_deref()).context("logout")?;
            println!("Logged out profile `{name}`");
        }
        AuthCommand::Refresh { profile } => {
            let runtime = resolve(
                &paths,
                &ResolveOverrides {
                    profile: profile.or(global_profile),
                    base_url: global_base_url,
                },
            )
            .context("resolve credentials for refresh")?;
            let client =
                Client::new(&runtime.base_url, &runtime.api_key).context("build API client")?;
            list_workspaces(
                &client,
                &ListWorkspacesParams {
                    page_size: Some(1),
                    ..ListWorkspacesParams::default()
                },
            )
            .context("validate credentials against the API")?;
            println!(
                "Credentials for profile `{}` are valid ({})",
                runtime.profile, runtime.base_url
            );
        }
        AuthCommand::Status => {
            let status = auth_status(&paths).context("read auth status")?;
            match status.active_profile.as_deref() {
                Some(name) => println!("Active profile: {name}"),
                None => println!("Active profile: (none)"),
            }
            println!("Base URL: {}", status.base_url);
            match status.api_key_masked.as_deref() {
                Some(masked) => println!("API key: {masked}"),
                None => println!("API key: (none)"),
            }
            match status.login_method.as_deref() {
                Some(method) => println!("Login method: {method}"),
                None => println!("Login method: (none)"),
            }
            println!("Logged in: {}", if status.logged_in { "yes" } else { "no" });
        }
        AuthCommand::Switch { profile } => {
            switch_profile(&paths, &profile)
                .with_context(|| format!("switch to profile `{profile}`"))?;
            println!("Switched active profile to `{profile}`");
        }
    }

    Ok(())
}
