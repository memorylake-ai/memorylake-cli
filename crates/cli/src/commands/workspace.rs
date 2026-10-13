//! `memorylake workspace` / `ws` commands.
//!
//! `workspace delete` erases everything a workspace holds, so it will not run
//! on an id alone: `--confirm` has to repeat the workspace's name, which the
//! command reads back before deleting. A caller that never looked at the
//! workspace cannot name it, and one that looked at the wrong one is caught.

use std::collections::BTreeMap;

use anyhow::{Context, Result, bail};
use clap::Subcommand;
use memorylake_core::api::workspaces::{
    CreateWorkspaceRequest, DEFAULT_WORKSPACE_CUSTOM_ID, ListWorkspacesParams,
    UpdateWorkspaceRequest, WORKSPACE_DESCRIPTION_MAX_CHARS, WORKSPACE_NAME_MAX_CHARS, Workspace,
    create_workspace, delete_workspace, get_workspace, get_workspace_by_custom_id, list_workspaces,
    update_workspace,
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
    /// Update a workspace's name, description or metadata.
    ///
    /// Only the flags you pass are sent; at least one is required.
    /// `--metadata` replaces the whole stored map rather than merging into
    /// it, so pass every key you want to keep; `--metadata '{}'` clears it.
    /// `--description ""` clears the description.
    Update {
        /// Workspace id.
        id: String,
        /// New display name (1–255 characters).
        #[arg(long, value_parser = parse_workspace_name)]
        name: Option<String>,
        /// New description (at most 2000 characters).
        #[arg(long, value_parser = parse_workspace_description)]
        description: Option<String>,
        /// Replacement metadata as a JSON object of strings,
        /// e.g. '{"team":"core"}'.
        #[arg(long, value_name = "JSON", value_parser = super::parse_string_map)]
        metadata: Option<BTreeMap<String, String>>,
    },
    /// Permanently delete a workspace and everything in it.
    ///
    /// Irreversible: its projects, documents and memories go with it. There
    /// is no prompt; instead `--confirm` must
    /// repeat the workspace's current name exactly, as `workspace get <id>`
    /// prints it, or nothing is deleted. The team's default workspace
    /// (custom_id `_sys_default_workspace`) is refused.
    ///
    /// A profile that remembered this workspace (`workspace use`) forgets it.
    Delete {
        /// Workspace id (`ws-...`, from `workspace list`).
        #[arg(value_parser = super::parse_non_blank)]
        id: String,
        /// The workspace's current name, repeated exactly.
        #[arg(long, value_name = "NAME", value_parser = parse_confirm_name)]
        confirm: String,
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
        // An empty update is a caller mistake the server would answer with
        // the unchanged workspace; say so before asking for credentials.
        WorkspaceCommand::Update {
            name: None,
            description: None,
            metadata: None,
            ..
        } => bail!("nothing to update; pass --name, --description or --metadata"),
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
        WorkspaceCommand::Update {
            id,
            name,
            description,
            metadata,
        } => {
            let data = update_workspace(
                &client,
                &id,
                &UpdateWorkspaceRequest {
                    name,
                    description,
                    metadata,
                },
            )
            .with_context(|| format!("update workspace `{id}`"))?;
            println!("{}", serde_json::to_string_pretty(&data)?);
        }
        WorkspaceCommand::Delete { id, confirm } => {
            let workspace = get_workspace(&client, &id)
                .with_context(|| format!("read workspace `{id}` to confirm against"))?;
            check_deletable(&workspace, &confirm)?;
            delete_workspace(&client, &id).with_context(|| format!("delete workspace `{id}`"))?;
            println!("Deleted workspace `{id}` ({})", workspace.name);
            for profile in forget_everywhere(&paths, &id)? {
                println!("profile `{profile}` no longer remembers it");
            }
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

/// Accept a workspace name the API will take: 1–255 characters.
fn parse_workspace_name(raw: &str) -> std::result::Result<String, String> {
    let chars = raw.chars().count();
    if chars == 0 || chars > WORKSPACE_NAME_MAX_CHARS {
        return Err(format!(
            "must be 1 to {WORKSPACE_NAME_MAX_CHARS} characters, got {chars}"
        ));
    }
    Ok(raw.to_string())
}

/// Accept a workspace description the API will take: at most 2000
/// characters. Empty is allowed and clears the description.
fn parse_workspace_description(raw: &str) -> std::result::Result<String, String> {
    let chars = raw.chars().count();
    if chars > WORKSPACE_DESCRIPTION_MAX_CHARS {
        return Err(format!(
            "must be at most {WORKSPACE_DESCRIPTION_MAX_CHARS} characters, got {chars}"
        ));
    }
    Ok(raw.to_string())
}

/// Accept a `--confirm` name: anything but blank, kept exactly as typed so
/// it is compared with the name character for character.
fn parse_confirm_name(raw: &str) -> std::result::Result<String, String> {
    if raw.trim().is_empty() {
        return Err("must repeat the workspace's name".to_string());
    }
    Ok(raw.to_string())
}

/// Refuse to delete `workspace` unless `confirm` repeats its name and it is
/// not the team's default workspace.
fn check_deletable(workspace: &Workspace, confirm: &str) -> Result<()> {
    if workspace.custom_id.as_deref() == Some(DEFAULT_WORKSPACE_CUSTOM_ID) {
        bail!(
            "refusing to delete `{}`: it is the team's default workspace \
             (custom_id {DEFAULT_WORKSPACE_CUSTOM_ID}), which other services look up",
            workspace.id
        );
    }
    if workspace.name != confirm {
        bail!(
            "refusing to delete `{}`: --confirm {confirm:?} does not match its name {:?}",
            workspace.id,
            workspace.name
        );
    }
    Ok(())
}

/// Forget a deleted workspace on every profile that remembered it, and name
/// those profiles.
///
/// A remembered id that no longer exists would fail every later command that
/// leans on it. Workspace ids are unique, so a match on another profile is the
/// same, now deleted, workspace.
fn forget_everywhere(paths: &Paths, id: &str) -> Result<Vec<String>> {
    let config = load_file_config(paths).context("read local config")?;
    let mut forgotten = Vec::new();
    for (profile, settings) in &config.profiles {
        if settings.workspace.as_deref() == Some(id) {
            clear_profile_workspace(paths, profile)
                .with_context(|| format!("forget the deleted workspace on profile `{profile}`"))?;
            forgotten.push(profile.clone());
        }
    }
    Ok(forgotten)
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
