//! `memorylake workspace` / `ws` commands.

use anyhow::{Context, Result, bail};
use clap::Subcommand;
use memorylake_core::api::workspaces::{
    CreateWorkspaceRequest, ListWorkspacesParams, Workspace, create_workspace, get_workspace,
    get_workspace_by_custom_id, list_workspaces,
};
use memorylake_core::{
    Client, DEFAULT_PROFILE, Paths, ResolveOverrides, clear_profile_workspace, load_file_config,
    resolve, resolve_profile_workspace, set_profile_workspace,
};

use crate::interactive::select_index;

/// Workspace subcommands.
#[derive(Debug, Subcommand)]
pub enum WorkspaceCommand {
    /// List workspaces.
    List {
        /// Number of items per page.
        #[arg(long)]
        page_size: Option<u32>,
        /// Continuation token from a previous response.
        #[arg(long)]
        continuation_token: Option<String>,
        /// Fuzzy filter by workspace name (partial match).
        #[arg(long = "name")]
        name_fuzzy: Option<String>,
    },
    /// Create a workspace.
    Create {
        /// Workspace display name.
        #[arg(long)]
        name: String,
        /// Caller-defined unique external id.
        #[arg(long)]
        custom_id: String,
        /// Optional description.
        #[arg(long)]
        description: Option<String>,
    },
    /// Get a single workspace by id.
    Get {
        /// Workspace id (or custom_id when `--by-custom-id` is set).
        id: String,
        /// Treat the positional argument as a caller-defined custom_id.
        #[arg(long)]
        by_custom_id: bool,
    },
    /// Remember a workspace so other commands can omit `--workspace`.
    ///
    /// Without an id this lists your workspaces and lets you pick one. The
    /// choice is stored on the active profile in `config.toml`; an explicit
    /// `--workspace` on any command still wins over it.
    Use {
        /// Workspace id to remember. Omit to choose from a list.
        id: Option<String>,
        /// Forget the remembered workspace instead of setting one.
        #[arg(long, conflicts_with = "id")]
        clear: bool,
    },
    /// Show which workspace commands will use, and where it came from.
    Current,
}

/// Execute a `workspace` subcommand.
pub fn run(
    command: WorkspaceCommand,
    profile: Option<String>,
    base_url: Option<String>,
) -> Result<()> {
    let paths = Paths::default_home().context("resolve MemoryLake config paths")?;

    // `current` and `use --clear` only read and write local config, so they run
    // before credentials are resolved. Requiring a login to answer "which
    // workspace is set?" would report a missing API key to someone asking a
    // question that has nothing to do with one.
    match &command {
        WorkspaceCommand::Current => {
            return report_current(&paths, profile.as_deref());
        }
        WorkspaceCommand::Use { clear: true, .. } => {
            return forget_workspace(&paths, profile.as_deref());
        }
        _ => {}
    }

    let runtime = resolve(&paths, &ResolveOverrides { profile, base_url })
        .context("resolve API credentials")?;
    let client = Client::new(&runtime.base_url, &runtime.api_key).context("build API client")?;

    match command {
        WorkspaceCommand::List {
            page_size,
            continuation_token,
            name_fuzzy,
        } => {
            let data = list_workspaces(
                &client,
                &ListWorkspacesParams {
                    page_size,
                    continuation_token,
                    name_fuzzy,
                },
            )
            .context("list workspaces")?;
            println!("{}", serde_json::to_string_pretty(&data)?);
        }
        WorkspaceCommand::Create {
            name,
            custom_id,
            description,
        } => {
            let data = create_workspace(
                &client,
                &CreateWorkspaceRequest {
                    name,
                    custom_id,
                    description,
                },
            )
            .context("create workspace")?;
            println!("{}", serde_json::to_string_pretty(&data)?);
        }
        WorkspaceCommand::Get { id, by_custom_id } => {
            let data = if by_custom_id {
                get_workspace_by_custom_id(&client, &id).context("get workspace by custom_id")?
            } else {
                get_workspace(&client, &id).context("get workspace")?
            };
            println!("{}", serde_json::to_string_pretty(&data)?);
        }
        WorkspaceCommand::Use { id, .. } => {
            // `--clear` returned earlier, before credentials were needed.
            let chosen = match id {
                Some(id) => verify_workspace(&client, &id)?,
                None => choose_workspace(&client)?,
            };
            set_profile_workspace(&paths, &runtime.profile, &chosen.id)
                .context("remember the chosen workspace")?;
            println!(
                "profile `{}` now uses {} ({})",
                runtime.profile, chosen.name, chosen.id
            );
        }
        // Handled before credentials were resolved.
        WorkspaceCommand::Current => unreachable!("handled without credentials"),
    }

    Ok(())
}

/// Resolve which profile to act on without requiring credentials.
///
/// `resolve` refuses when nobody is logged in, which is the right answer for a
/// command that needs the API but the wrong one for reading local config.
fn profile_name(paths: &Paths, cli_profile: Option<&str>) -> Result<String> {
    if let Some(profile) = cli_profile {
        return Ok(profile.to_string());
    }
    let config = load_file_config(paths).context("read local config")?;
    Ok(config
        .active_profile
        .unwrap_or_else(|| DEFAULT_PROFILE.to_string()))
}

/// Print which workspace commands will use, and where it came from.
fn report_current(paths: &Paths, cli_profile: Option<&str>) -> Result<()> {
    let profile = profile_name(paths, cli_profile)?;
    match resolve_profile_workspace(paths, &profile, None)
        .context("resolve the current workspace")?
    {
        Some((workspace, source)) => println!("{workspace} (from {source})"),
        None => {
            println!("no workspace set for profile `{profile}`");
            println!("pick one with: memorylake workspace use");
        }
    }
    Ok(())
}

/// Forget the remembered workspace for a profile.
fn forget_workspace(paths: &Paths, cli_profile: Option<&str>) -> Result<()> {
    let profile = profile_name(paths, cli_profile)?;
    clear_profile_workspace(paths, &profile).context("clear the remembered workspace")?;
    println!("profile `{profile}` no longer remembers a workspace");
    Ok(())
}

/// Confirm an explicitly named workspace exists before remembering it.
///
/// Storing an id the API rejects would turn one clear error now into a
/// confusing one on every later command.
fn verify_workspace(client: &Client, id: &str) -> Result<Workspace> {
    get_workspace(client, id)
        .with_context(|| format!("look up workspace `{id}` before remembering it"))
}

/// List the caller's workspaces and let them pick one.
///
/// Asking for a choice rather than an id is the point: a fresh user has no id
/// to type, and this is the path the installer walks them through.
fn choose_workspace(client: &Client) -> Result<Workspace> {
    let page = list_workspaces(
        client,
        &ListWorkspacesParams {
            // One page is enough to choose from; an account with more than this
            // is better served by `workspace use <id>` after a filtered `list`.
            page_size: Some(50),
            continuation_token: None,
            name_fuzzy: None,
        },
    )
    .context("list workspaces to choose from")?;

    match page.items.len() {
        0 => bail!(
            "no workspaces found for this account\n\
             create one first: memorylake workspace create --name \"My Workspace\" --custom-id my-ws-001"
        ),
        // Nothing to choose between — take it, and say so rather than
        // presenting a one-item menu.
        1 => {
            let only = page.items.into_iter().next().expect("length checked");
            println!("only one workspace available: {} ({})", only.name, only.id);
            Ok(only)
        }
        _ => {
            let labels: Vec<String> = page
                .items
                .iter()
                .map(|workspace| format!("{} ({})", workspace.name, workspace.id))
                .collect();
            let refs: Vec<&str> = labels.iter().map(String::as_str).collect();
            let index = select_index("Select a workspace", &refs, 0)?;
            Ok(page
                .items
                .into_iter()
                .nth(index)
                .expect("index came from this list"))
        }
    }
}
