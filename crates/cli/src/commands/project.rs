//! `memorylake project` / `proj` commands.

mod document;

use anyhow::{Context, Result};
use clap::Subcommand;
use memorylake_core::api::projects::{
    CreateProjectRequest, ListProjectsParams, UpdateProjectRequest, create_project, delete_project,
    get_project, get_project_by_custom_id, list_projects, update_project,
};
use memorylake_core::{Client, Paths, ResolveOverrides, resolve};

use document::{DocumentCommand, run as run_document};

/// Project subcommands.
///
/// Projects live inside a workspace, so every subcommand takes `--workspace`.
/// There is no default or remembered workspace.
#[derive(Debug, Subcommand)]
pub enum ProjectCommand {
    /// List projects in a workspace.
    List {
        /// Workspace id that owns the projects.
        #[arg(long)]
        workspace: String,
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
        #[arg(long)]
        workspace: String,
        /// Project display name.
        #[arg(long)]
        name: String,
        /// Caller-defined external id. Must be unique within the workspace.
        #[arg(long)]
        custom_id: String,
        /// Optional description.
        #[arg(long)]
        description: Option<String>,
    },
    /// Get a single project by id.
    Get {
        /// Workspace id that owns the project.
        #[arg(long)]
        workspace: String,
        /// Project id (or custom_id when `--by-custom-id` is set).
        id: String,
        /// Treat the positional argument as a caller-defined custom_id.
        #[arg(long)]
        by_custom_id: bool,
    },
    /// Update a project's name or description.
    ///
    /// Only the flags you pass are sent; omitted fields are left unchanged.
    Update {
        /// Workspace id that owns the project.
        #[arg(long)]
        workspace: String,
        /// Project id.
        id: String,
        /// New display name.
        #[arg(long)]
        name: Option<String>,
        /// New description.
        #[arg(long)]
        description: Option<String>,
    },
    /// Permanently delete a project.
    ///
    /// The project and all of its documents and conversations are removed.
    /// This cannot be undone, and the command does not ask for confirmation.
    Delete {
        /// Workspace id that owns the project.
        #[arg(long)]
        workspace: String,
        /// Project id.
        id: String,
    },
    /// Manage the Library files imported into a project.
    #[command(visible_alias = "doc")]
    Document {
        #[command(subcommand)]
        command: DocumentCommand,
    },
}

/// Execute a `project` subcommand.
pub fn run(
    command: ProjectCommand,
    profile: Option<String>,
    base_url: Option<String>,
) -> Result<()> {
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
        } => {
            let data = create_project(
                &client,
                &workspace,
                &CreateProjectRequest {
                    name,
                    custom_id,
                    description,
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
        } => {
            let data = update_project(
                &client,
                &workspace,
                &id,
                &UpdateProjectRequest { name, description },
            )
            .context("update project")?;
            println!("{}", serde_json::to_string_pretty(&data)?);
        }
        ProjectCommand::Delete { workspace, id } => {
            delete_project(&client, &workspace, &id).context("delete project")?;
            println!("Deleted project `{id}` in workspace `{workspace}`");
        }
        ProjectCommand::Document { command } => run_document(&client, command)?,
    }

    Ok(())
}
