//! `memorylake auth` commands.

mod browser;
mod expiry;
mod interactive;

use std::io::IsTerminal;
use std::time::Duration;

use anyhow::{Context, Result, bail};
use clap::Subcommand;
use memorylake_core::api::workspaces::{ListWorkspacesParams, list_workspaces};
use memorylake_core::browser_login::{LoginClient, Revocation};
use memorylake_core::{
    BaseUrlSource, Client, DEFAULT_PROFILE, ENV_API_KEY, Error as CoreError, Paths,
    ResolveOverrides, StoredKey, auth_status, login_api_key, login_issued_key, logout, resolve,
    resolve_profile_base_url, stored_key, switch_profile,
};

use interactive::{InteractiveLoginMethod, prompt_api_key, prompt_base_url, prompt_login_method};

/// Authentication subcommands.
#[derive(Debug, Subcommand)]
pub enum AuthCommand {
    /// Authenticate and store credentials for a profile.
    ///
    /// With `--api-key`, stores that key without asking anything.
    /// With `--browser` (or `--no-browser`), logs in through the browser:
    /// approve the login there and MemoryLake creates an API key for this
    /// machine, so no key has to be copied by hand.
    /// With neither, asks for the endpoint and the method.
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
        /// Log in through the browser (skips method picker).
        #[arg(long, conflicts_with = "api_key")]
        browser: bool,
        /// Log in through the browser, but only print the login URL instead of
        /// opening it. For a program that opens the URL itself, or a machine
        /// whose browser is elsewhere. The URL is the first line on standard
        /// output; the command then waits for the login to be approved.
        #[arg(long, conflicts_with = "api_key")]
        no_browser: bool,
        /// Name for the API key a browser login creates, as the console will
        /// list it (defaults to `memorylake CLI · <hostname>`).
        #[arg(long, value_name = "NAME", conflicts_with = "api_key")]
        key_name: Option<String>,
        /// Seconds a browser login waits to be approved.
        #[arg(long, value_name = "SECONDS", default_value_t = browser::DEFAULT_TIMEOUT_SECS)]
        timeout: u64,
    },
    /// Remove stored credentials for a profile.
    ///
    /// An API key that a browser login created is revoked as well: it was made
    /// for this login, and nothing else has a copy.
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
            browser,
            no_browser,
            key_name,
            timeout,
        } => {
            run_login(
                &paths,
                LoginArgs {
                    api_key,
                    profile: profile.or(global_profile),
                    base_url: base_url.or(global_base_url),
                    browser,
                    no_browser,
                    key_name,
                    timeout: Duration::from_secs(timeout),
                },
            )?;
        }
        AuthCommand::Logout { profile } => {
            let profile = profile.or(global_profile);
            // Read before the credentials are gone: the key is what revokes
            // itself.
            let issued = issued_key(&paths, profile.as_deref());
            if let Some(stored) = &issued {
                report_revocation(stored, revoke(stored));
            }
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
            let status =
                auth_status(&paths, global_profile.as_deref()).context("read auth status")?;
            match (&global_profile, status.active_profile.as_deref()) {
                (Some(name), active) => println!(
                    "Profile: {name} (from --profile; active profile: {})",
                    active.unwrap_or("(none)")
                ),
                (None, Some(name)) => println!("Active profile: {name}"),
                (None, None) => println!("Active profile: (none)"),
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
            match (status.login_method.as_deref(), &status.issued) {
                // Stored as `api_key` so older binaries can use it; shown as
                // what the user did.
                (Some(_), Some(_)) => println!("Login method: browser"),
                (Some(method), None) => println!("Login method: {method}"),
                (None, _) => println!("Login method: (none)"),
            }
            let now = expiry::now();
            if let Some(issued) = &status.issued {
                println!("Key name: {}", issued.name);
                if let Some(team) = &issued.tenant_id {
                    println!("Team: {team}");
                }
                if let Some(expires_at) = issued.expires_at {
                    println!("Key expires: {}", expiry::describe(expires_at, now));
                }
            }
            println!("Logged in: {}", if status.logged_in { "yes" } else { "no" });
            if let Some(warning) = status
                .issued
                .as_ref()
                .and_then(|issued| issued.expires_at)
                .and_then(|expires_at| expiry::warning(expires_at, now))
            {
                println!("Hint: {warning}");
            }
            if status.profile.is_none()
                && std::env::var(ENV_API_KEY).is_ok_and(|key| !key.trim().is_empty())
            {
                println!(
                    "Hint: {ENV_API_KEY} is set, but an environment variable alone does not \
                     create a session; run `memorylake auth login --api-key \"${ENV_API_KEY}\"`"
                );
            }

            if status.logged_in {
                // Validate the same profile `auth_status` displayed.
                let runtime = resolve(
                    &paths,
                    &ResolveOverrides {
                        profile: status.profile.clone(),
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

/// What `auth login` was asked to do.
struct LoginArgs {
    api_key: Option<String>,
    profile: Option<String>,
    base_url: Option<String>,
    browser: bool,
    no_browser: bool,
    key_name: Option<String>,
    timeout: Duration,
}

fn run_login(paths: &Paths, args: LoginArgs) -> Result<()> {
    let LoginArgs {
        api_key,
        profile,
        base_url,
        browser,
        no_browser,
        key_name,
        timeout,
    } = args;
    let profile = profile.unwrap_or_else(|| DEFAULT_PROFILE.to_string());
    // Same precedence as runtime resolve: CLI → profile config → env → default.
    let (resolved_url, url_source) = resolve_profile_base_url(paths, &profile, base_url.as_deref())
        .context("resolve base URL for login")?;

    // Any of the browser flags picks the method, so no picker is shown.
    let browser_requested = browser || no_browser || key_name.is_some();

    // Offer the endpoint choice only when nothing chose one: a `--base-url`, a
    // profile entry or MEMORYLAKE_BASE_URL is an answer already given, and
    // re-asking would invite overwriting it by accident. `--api-key` means
    // non-interactive, so it never prompts either. A browser login asks only
    // when someone is there to answer: started by another program it has no
    // terminal, and must get as far as printing its URL.
    let ask_endpoint = url_source == BaseUrlSource::Default
        && api_key.is_none()
        && (!browser_requested || std::io::stdin().is_terminal());
    let probe_url = if ask_endpoint {
        prompt_base_url()?
    } else {
        resolved_url
    };

    let method = if api_key.is_some() {
        InteractiveLoginMethod::ApiKey
    } else if browser_requested {
        InteractiveLoginMethod::Browser
    } else {
        prompt_login_method(&probe_url)?
    };

    // A key that a browser login made for this profile is about to be
    // forgotten. Looked up before anything is overwritten.
    let replaced = issued_key(paths, Some(&profile));

    let new_key = match method {
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
            api_key
        }
        InteractiveLoginMethod::Browser => {
            let created = browser::run(&browser::BrowserLogin {
                base_url: &probe_url,
                key_name: key_name.as_deref(),
                open_browser: !no_browser,
                timeout,
            })?;
            // Stored as soon as it exists, without a trial request first: the
            // key is already counted against the account, and it cannot be
            // fetched a second time.
            login_issued_key(
                paths,
                &profile,
                &created.key,
                &probe_url,
                created.issued.clone(),
            )
            .with_context(|| format!("login for profile `{profile}`"))?;
            println!("Logged in to profile `{profile}` using browser");
            match created.issued.expires_at {
                Some(expires_at) => eprintln!(
                    "Created API key `{}`, valid until {}.",
                    created.issued.name,
                    expiry::utc_date(expires_at)
                ),
                None => eprintln!("Created API key `{}`.", created.issued.name),
            }
            created.key
        }
    };

    // Only once the new login is in place, so a login that fails leaves the
    // old one working. Quiet on failure: the new login succeeded, and the old
    // key runs out on its own.
    if let Some(old) = replaced.filter(|old| old.api_key != new_key)
        && matches!(revoke(&old), Ok(Revocation::Revoked))
    {
        eprintln!("Revoked the API key this login replaces.");
    }

    Ok(())
}

/// The stored key for `profile`, when a browser login created it.
///
/// Only such a key is revoked on the way out: it was made for one login on
/// this machine. A key the user pasted in is theirs to manage, and may be in
/// use elsewhere. Failing to read the files means there is nothing to revoke;
/// the command that asked reports that failure itself.
fn issued_key(paths: &Paths, profile: Option<&str>) -> Option<StoredKey> {
    stored_key(paths, profile)
        .ok()
        .flatten()
        .filter(|stored| stored.issued.is_some())
}

fn revoke(stored: &StoredKey) -> memorylake_core::Result<Revocation> {
    LoginClient::new(&stored.base_url)?.revoke_api_key(&stored.api_key)
}

/// Say how revoking the key went. Never fatal: `auth logout` removes the local
/// credentials whatever the server answered.
fn report_revocation(stored: &StoredKey, outcome: memorylake_core::Result<Revocation>) {
    let name = stored
        .issued
        .as_ref()
        .map_or("(unnamed)", |issued| issued.name.as_str());
    match outcome {
        Ok(Revocation::Revoked) => eprintln!("Revoked API key `{name}`."),
        Ok(Revocation::NotIssuedByLogin) => eprintln!(
            "API key `{name}` was not revoked: the server does not count it as created by a \
             login. It is still valid; remove it in the console if it is no longer needed."
        ),
        Err(err) => eprintln!(
            "Could not revoke API key `{name}`: {err}\n\
             It is still valid; remove it in the console if it is no longer needed."
        ),
    }
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
