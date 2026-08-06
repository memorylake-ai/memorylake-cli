//! `memorylake workspace` / `ws` commands.

use anyhow::{Context, Result};
use clap::Subcommand;
use memorylake_core::api::workspaces::{
    CreateWorkspaceRequest, ListWorkspacesParams, create_workspace, get_workspace,
    get_workspace_by_custom_id, list_workspaces,
};
use memorylake_core::{Client, Paths, ResolveOverrides, resolve};

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
}

/// Execute a `workspace` subcommand.
pub fn run(
    command: WorkspaceCommand,
    profile: Option<String>,
    base_url: Option<String>,
) -> Result<()> {
    let paths = Paths::default_home().context("resolve MemoryLake config paths")?;
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
    }

    Ok(())
}
