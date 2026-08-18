//! `memorylake auth` commands.

mod interactive;

use anyhow::{Context, Result, bail};
use clap::Subcommand;
use memorylake_core::api::workspaces::{ListWorkspacesParams, list_workspaces};
use memorylake_core::{
    BaseUrlSource, Client, DEFAULT_PROFILE, Error as CoreError, Paths, ResolveOverrides,
    auth_status, login_api_key, logout, resolve, resolve_profile_base_url, switch_profile,
};

use interactive::{InteractiveLoginMethod, prompt_api_key, prompt_base_url, prompt_login_method};

/// Authentication subcommands.
#[derive(Debug, Subcommand)]
pub enum AuthCommand {
    /// Authenticate and store credentials for a profile.
    ///
    /// With `--api-key`, logs in non-interactively using API key auth.
    /// Without `--api-key`, prompts to choose a login method (`api_key` or `oauth`).
    Login {
        /// API key (`sk-...`). Implies API-key login (skips method picker).
        #[arg(long = "api-key")]
        api_key: Option<String>,
        /// Profile to write (defaults to `default`).
        #[arg(long)]
        profile: Option<String>,
        /// Base URL to store on the profile.
        #[arg(long)]
        base_url: Option<String>,
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
    ///
    /// When credentials are present, also validates them against the API.
    Status,
    /// Switch the active profile.
    ///
    /// Validates the target profile's credentials against the API before switching.
    Switch {
        /// Profile name to activate.
        profile: String,
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
        AuthCommand::Login {
            api_key,
            profile,
            base_url,
        } => {
            run_login(
                &paths,
                api_key,
                profile.or(global_profile),
                base_url.or(global_base_url),
            )?;
        }
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
            validate_client(&client)?;
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
            println!("Base URL source: {}", status.base_url_source);
            match status.api_key_masked.as_deref() {
                Some(masked) => println!("API key: {masked}"),
                None => println!("API key: (none)"),
            }
            match status.api_key_source {
                Some(source) => println!("API key source: {source}"),
                None => println!("API key source: (none)"),
            }
            match status.login_method.as_deref() {
                Some(method) => println!("Login method: {method}"),
                None => println!("Login method: (none)"),
            }
            println!("Logged in: {}", if status.logged_in { "yes" } else { "no" });

            if status.logged_in {
                // Validate the same profile `auth_status` displayed (active), not a
                // global `--profile` override that would drift from the printed summary.
                let runtime = resolve(
                    &paths,
                    &ResolveOverrides {
                        profile: status.active_profile.clone(),
                        base_url: global_base_url,
                    },
                )
                .context("resolve credentials for status")?;
                let client = Client::new(&runtime.base_url, &runtime.api_key)
                    .context("build API client for status")?;
                validate_client(&client)?;
                println!("Credentials: valid");
            }
        }
        AuthCommand::Switch { profile } => {
            let runtime = resolve(
                &paths,
                &ResolveOverrides {
                    profile: Some(profile.clone()),
                    base_url: global_base_url,
                },
            )
            .with_context(|| format!("resolve credentials for profile `{profile}`"))?;
            let client = Client::new(&runtime.base_url, &runtime.api_key)
                .context("build API client for switch")?;
            validate_client(&client)?;
            switch_profile(&paths, &profile)
                .with_context(|| format!("switch to profile `{profile}`"))?;
            println!("Switched active profile to `{profile}`");
        }
    }

    Ok(())
}

fn run_login(
    paths: &Paths,
    api_key: Option<String>,
    profile: Option<String>,
    base_url: Option<String>,
) -> Result<()> {
    let profile = profile.unwrap_or_else(|| DEFAULT_PROFILE.to_string());
    // Same precedence as runtime resolve: CLI → profile config → env → default.
    let (resolved_url, url_source) = resolve_profile_base_url(paths, &profile, base_url.as_deref())
        .context("resolve base URL for login")?;

    // Offer the endpoint choice only when nothing chose one: a `--base-url`, a
    // profile entry or MEMORYLAKE_BASE_URL is an answer already given, and
    // re-asking would invite overwriting it by accident. `--api-key` means
    // non-interactive, so it never prompts either.
    let interactive = api_key.is_none();
    let probe_url = if interactive && url_source == BaseUrlSource::Default {
        prompt_base_url()?
    } else {
        resolved_url
    };

    let method = if api_key.is_some() {
        InteractiveLoginMethod::ApiKey
    } else {
        prompt_login_method(&probe_url)?
    };

    match method {
        InteractiveLoginMethod::ApiKey => {
            let api_key = match api_key {
                Some(key) if !key.trim().is_empty() => key.trim().to_string(),
                Some(_) => bail!("API key must not be empty"),
                None => prompt_api_key()?,
            };
            let client = Client::new(&probe_url, &api_key).context("build API client for login")?;
            validate_client(&client).map_err(|err| {
                anyhow::Error::new(err)
                    .context(format!("could not verify API key against {probe_url}"))
            })?;
            // Persist the URL we validated so stored config matches the probe.
            login_api_key(paths, &profile, &api_key, Some(&probe_url))
                .with_context(|| format!("login for profile `{profile}`"))?;
            println!("Logged in to profile `{profile}` using api_key");
        }
        InteractiveLoginMethod::OAuth => {
            // OAuth device/browser flow is not wired to the MemoryLake API yet.
            bail!(
                "OAuth login is not implemented yet; use `memorylake auth login --api-key <KEY>` or choose API key interactively"
            );
        }
    }

    Ok(())
}

fn validate_client(client: &Client) -> std::result::Result<(), CoreError> {
    list_workspaces(
        client,
        &ListWorkspacesParams {
            page_size: Some(1),
            ..ListWorkspacesParams::default()
        },
    )?;
    Ok(())
}
