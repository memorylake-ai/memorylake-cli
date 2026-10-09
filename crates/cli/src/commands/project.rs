//! `memorylake project` / `proj` commands.

mod database;
mod document;

use anyhow::{Context, Result};
use clap::Subcommand;
use memorylake_core::api::projects::{
    CreateProjectRequest, ListProjectsParams, UpdateProjectRequest, create_project, delete_project,
    get_project, get_project_by_custom_id, get_project_statistics, list_projects, update_project,
};
use memorylake_core::{Client, Paths, ResolveOverrides, resolve};

use super::require_workspace;
use super::search::{IdList, parse_id_list};
use database::{DatabaseCommand, run as run_database};
use document::{DocumentCommand, run as run_document};

/// Project subcommands.
///
/// Projects live inside a workspace, so every subcommand needs one: either
/// `--workspace` or the one remembered by `workspace use`.
#[derive(Debug, Subcommand)]
pub enum ProjectCommand {
    /// List projects in a workspace.
    List {
        /// Workspace id that owns the projects.
        ///
        /// Defaults to the workspace remembered by `workspace use`.
        #[arg(long)]
        workspace: Option<String>,
        /// Number of items per page.
        #[arg(long)]
        page_size: Option<u32>,
        /// Continuation token from a previous response.
        #[arg(long)]
        continuation_token: Option<String>,
        /// Fuzzy filter by project name (partial match).
        #[arg(long = "name")]
        name_fuzzy: Option<String>,
    },
    /// Create a project.
    Create {
        /// Workspace id to create the project in.
        ///
        /// Defaults to the workspace remembered by `workspace use`.
        #[arg(long)]
        workspace: Option<String>,
        /// Project display name.
        #[arg(long)]
        name: String,
        /// Caller-defined external id. Must be unique within the workspace.
        #[arg(long)]
        custom_id: String,
        /// Optional description.
        #[arg(long)]
        description: Option<String>,
        /// Industry opendata ids to attach, comma-separated, e.g.
        /// `research/academic,financial/markets`. See `industry list`.
        #[arg(long, value_name = "IDS", value_parser = parse_id_list)]
        industry_ids: Option<IdList>,
    },
    /// Get a single project by id.
    Get {
        /// Workspace id that owns the project.
        ///
        /// Defaults to the workspace remembered by `workspace use`.
        #[arg(long)]
        workspace: Option<String>,
        /// Project id (or custom_id when `--by-custom-id` is set).
        id: String,
        /// Treat the positional argument as a caller-defined custom_id.
        #[arg(long)]
        by_custom_id: bool,
    },
    /// Update a project's name, description, or industries.
    ///
    /// Only the flags you pass are sent; omitted fields are left unchanged.
    Update {
        /// Workspace id that owns the project.
        ///
        /// Defaults to the workspace remembered by `workspace use`.
        #[arg(long)]
        workspace: Option<String>,
        /// Project id.
        id: String,
        /// New display name.
        #[arg(long)]
        name: Option<String>,
        /// New description.
        #[arg(long)]
        description: Option<String>,
        /// New industry opendata ids, comma-separated. REPLACES the attached
        /// set — list every industry you want to keep.
        #[arg(long, value_name = "IDS", value_parser = parse_id_list, conflicts_with = "clear_industries")]
        industry_ids: Option<IdList>,
        /// Detach every industry from the project.
        #[arg(long)]
        clear_industries: bool,
    },
    /// Permanently delete a project.
    ///
    /// The project and all of its documents and conversations are removed.
    /// This cannot be undone, and the command does not ask for confirmation.
    Delete {
        /// Workspace id that owns the project.
        ///
        /// Defaults to the workspace remembered by `workspace use`.
        #[arg(long)]
        workspace: Option<String>,
        /// Project id.
        id: String,
    },
    /// Show how many documents and databases a project holds, and how many
    /// of each are pending, running, okay or in error.
    #[command(visible_alias = "statistics")]
    Stats {
        /// Workspace id that owns the project.
        ///
        /// Defaults to the workspace remembered by `workspace use`.
        #[arg(long)]
        workspace: Option<String>,
        /// Project id (custom_ids are not accepted here).
        id: String,
    },
    /// Manage the Library files imported into a project.
    #[command(visible_alias = "doc")]
    Document {
        #[command(subcommand)]
        command: DocumentCommand,
    },
    /// Manage the databases a project uses as memory sources.
    #[command(visible_alias = "db")]
    Database {
        #[command(subcommand)]
        command: DatabaseCommand,
    },
}

/// Execute a `project` subcommand.
pub fn run(
    command: ProjectCommand,
    profile: Option<String>,
    base_url: Option<String>,
) -> Result<()> {
    // Database memory commands read and check their input before resolving
    // credentials, so they resolve their own.
    let command = match command {
        ProjectCommand::Database { command } => return run_database(command, profile, base_url),
        other => other,
    };

    let paths = Paths::default_home().context("resolve MemoryLake config paths")?;
    let runtime = resolve(&paths, &ResolveOverrides { profile, base_url })
        .context("resolve API credentials")?;
    let client = Client::new(&runtime.base_url, &runtime.api_key).context("build API client")?;

    match command {
        ProjectCommand::List {
            workspace,
            page_size,
            continuation_token,
            name_fuzzy,
        } => {
            let workspace = require_workspace(&paths, &runtime.profile, workspace)?;
            let data = list_projects(
                &client,
                &workspace,
                &ListProjectsParams {
                    page_size,
                    continuation_token,
                    name_fuzzy,
                },
            )
            .context("list projects")?;
            println!("{}", serde_json::to_string_pretty(&data)?);
        }
        ProjectCommand::Create {
            workspace,
            name,
            custom_id,
            description,
            industry_ids,
        } => {
            let workspace = require_workspace(&paths, &runtime.profile, workspace)?;
            let data = create_project(
                &client,
                &workspace,
                &CreateProjectRequest {
                    name,
                    custom_id,
                    description,
                    industry_ids: industry_ids.map(|list| list.0),
                },
            )
            .context("create project")?;
            println!("{}", serde_json::to_string_pretty(&data)?);
        }
        ProjectCommand::Get {
            workspace,
            id,
            by_custom_id,
        } => {
            let workspace = require_workspace(&paths, &runtime.profile, workspace)?;
            let data = if by_custom_id {
                get_project_by_custom_id(&client, &workspace, &id)
                    .context("get project by custom_id")?
            } else {
                get_project(&client, &workspace, &id).context("get project")?
            };
            println!("{}", serde_json::to_string_pretty(&data)?);
        }
        ProjectCommand::Update {
            workspace,
            id,
            name,
            description,
            industry_ids,
            clear_industries,
        } => {
            let workspace = require_workspace(&paths, &runtime.profile, workspace)?;
            // `--clear-industries` sends an empty list, which the API reads as
            // "detach all"; leaving both flags out sends nothing and keeps them.
            let industry_ids = match industry_ids {
                Some(list) => Some(list.0),
                None => clear_industries.then(Vec::new),
            };
            let data = update_project(
                &client,
                &workspace,
                &id,
                &UpdateProjectRequest {
                    name,
                    description,
                    industry_ids,
                },
            )
            .context("update project")?;
            println!("{}", serde_json::to_string_pretty(&data)?);
        }
        ProjectCommand::Delete { workspace, id } => {
            let workspace = require_workspace(&paths, &runtime.profile, workspace)?;
            delete_project(&client, &workspace, &id).context("delete project")?;
            println!("Deleted project `{id}` in workspace `{workspace}`");
        }
        ProjectCommand::Stats { workspace, id } => {
            let workspace = require_workspace(&paths, &runtime.profile, workspace)?;
            let data = get_project_statistics(&client, &workspace, &id)
                .with_context(|| format!("get statistics of project `{id}`"))?;
            println!("{}", serde_json::to_string_pretty(&data)?);
        }
        ProjectCommand::Document { command } => {
            run_document(&client, &paths, &runtime.profile, command)?
        }
        ProjectCommand::Database { .. } => unreachable!("dispatched before resolving credentials"),
    }

    Ok(())
}
