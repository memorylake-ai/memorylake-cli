//! `memorylake project database` / `proj db` commands.
//!
//! A database memory makes a workspace datasource (`datasource`) available to
//! a project as a memory source, with an optional `instruction` on how
//! questions against it should be answered.

use std::path::PathBuf;

use anyhow::{Context, Result, bail};
use clap::{Args, Subcommand};
use memorylake_core::api::databases::{
    CreateDatabaseMemoryRequest, ListDatabaseMemoriesParams, UpdateDatabaseMemoryRequest,
    create_database_memory, delete_database_memory, generate_database_instruction,
    get_database_memory, list_database_memories, reload_database_memory, update_database_memory,
};
use memorylake_core::{Client, Paths, ResolveOverrides, resolve};

use crate::commands::input::read_file_or_stdin;
use crate::commands::{print_json, require_workspace};

/// Workspace and project every subcommand addresses.
#[derive(Debug, Args)]
pub struct ProjectArgs {
    /// Workspace id that owns the project.
    ///
    /// Defaults to the workspace remembered by `workspace use`.
    #[arg(long)]
    pub workspace: Option<String>,
    /// Project id.
    #[arg(long)]
    pub project: String,
}

/// `--instruction` or `--instruction-file`, at most one.
#[derive(Debug, Args)]
#[group(id = "instruction_source", multiple = false)]
pub struct InstructionArgs {
    /// Guidance for answering questions against this database: what the data
    /// means, which tables matter, house conventions.
    #[arg(long)]
    pub instruction: Option<String>,
    /// Read the instruction from this file; `-` reads standard input.
    #[arg(long, value_name = "PATH")]
    pub instruction_file: Option<PathBuf>,
}

impl InstructionArgs {
    /// The instruction text, read from its file when one was named.
    ///
    /// File content is sent as-is, trailing newline included: it is prose,
    /// and nothing in it is a delimiter.
    fn read(self) -> Result<Option<String>> {
        match (self.instruction, self.instruction_file) {
            (Some(text), _) => Ok(Some(text)),
            (None, Some(path)) => read_file_or_stdin(&path, "instruction file").map(Some),
            (None, None) => Ok(None),
        }
    }
}

/// `project database` subcommands.
#[derive(Debug, Subcommand)]
pub enum DatabaseCommand {
    /// List the database memories in a project.
    ///
    /// `instruction` is not included in list items; use `get` to read it.
    List {
        #[command(flatten)]
        target: ProjectArgs,
        /// Number of items per page (1-100).
        #[arg(long, value_parser = clap::value_parser!(u32).range(1..=100))]
        page_size: Option<u32>,
        /// Continuation token from a previous response.
        #[arg(long)]
        continuation_token: Option<String>,
    },
    /// Add a workspace datasource to a project as a database memory.
    Create {
        #[command(flatten)]
        target: ProjectArgs,
        /// Datasource to read (see `datasource list`). Must be in the
        /// project's workspace.
        #[arg(long, value_name = "DATASOURCE_ID")]
        datasource: String,
        /// Display name.
        #[arg(long)]
        name: String,
        #[command(flatten)]
        instruction: InstructionArgs,
        /// Analysis model to draw knowledge from; must be built on the same
        /// datasource.
        #[arg(long, value_name = "ANALYSIS_MODEL_ID")]
        analysis_model: Option<String>,
    },
    /// Get a database memory, including its instruction.
    Get {
        #[command(flatten)]
        target: ProjectArgs,
        /// Database memory id.
        database_id: String,
    },
    /// Update a database memory's name, instruction, or analysis model.
    ///
    /// Only the flags you pass are sent. The datasource cannot be changed.
    Update {
        #[command(flatten)]
        target: ProjectArgs,
        /// Database memory id.
        database_id: String,
        /// New display name.
        #[arg(long)]
        name: Option<String>,
        #[command(flatten)]
        instruction: InstructionArgs,
        /// Bind this analysis model instead of the current one.
        #[arg(
            long,
            value_name = "ANALYSIS_MODEL_ID",
            conflicts_with = "clear_analysis_model"
        )]
        analysis_model: Option<String>,
        /// Remove the analysis model binding.
        #[arg(long)]
        clear_analysis_model: bool,
    },
    /// Remove a database memory from a project.
    ///
    /// The datasource is kept. There is no confirmation prompt. The server
    /// reports success for an id that does not exist, too.
    Delete {
        #[command(flatten)]
        target: ProjectArgs,
        /// Database memory id.
        database_id: String,
    },
    /// Refresh the database's schema and table metadata.
    Reload {
        #[command(flatten)]
        target: ProjectArgs,
        /// Database memory id.
        database_id: String,
    },
    /// Draft an instruction for a database memory. Nothing is saved.
    ///
    /// Prints `{"instruction": "..."}`. To keep the draft, pass it to
    /// `update --instruction-file -`, e.g.
    ///
    ///   memorylake project database generate-instruction --project P DB \
    ///     | jq -r .instruction \
    ///     | memorylake project database update --project P DB --instruction-file -
    GenerateInstruction {
        #[command(flatten)]
        target: ProjectArgs,
        /// Database memory id.
        database_id: String,
    },
}

/// Execute a `project database` subcommand.
///
/// Instruction files are read, and empty updates refused, before credentials
/// are resolved.
pub fn run(
    command: DatabaseCommand,
    profile: Option<String>,
    base_url: Option<String>,
) -> Result<()> {
    let command = prepare(command)?;

    let paths = Paths::default_home().context("resolve MemoryLake config paths")?;
    let runtime = resolve(&paths, &ResolveOverrides { profile, base_url })
        .context("resolve API credentials")?;
    let client = Client::new(&runtime.base_url, &runtime.api_key).context("build API client")?;
    let workspace_of = |flag: Option<String>| require_workspace(&paths, &runtime.profile, flag);

    execute(&client, workspace_of, command)
}

/// A command with its local input read and checked.
enum Prepared {
    List {
        target: ProjectArgs,
        params: ListDatabaseMemoriesParams,
    },
    Create {
        target: ProjectArgs,
        request: CreateDatabaseMemoryRequest,
    },
    Get {
        target: ProjectArgs,
        database_id: String,
    },
    Update {
        target: ProjectArgs,
        database_id: String,
        request: UpdateDatabaseMemoryRequest,
    },
    Delete {
        target: ProjectArgs,
        database_id: String,
    },
    Reload {
        target: ProjectArgs,
        database_id: String,
    },
    GenerateInstruction {
        target: ProjectArgs,
        database_id: String,
    },
}

fn prepare(command: DatabaseCommand) -> Result<Prepared> {
    Ok(match command {
        DatabaseCommand::List {
            target,
            page_size,
            continuation_token,
        } => Prepared::List {
            target,
            params: ListDatabaseMemoriesParams {
                page_size,
                continuation_token,
            },
        },
        DatabaseCommand::Create {
            target,
            datasource,
            name,
            instruction,
            analysis_model,
        } => Prepared::Create {
            target,
            request: CreateDatabaseMemoryRequest {
                name,
                db_datasource_id: datasource,
                instruction: instruction.read()?,
                analysis_model_id: analysis_model,
            },
        },
        DatabaseCommand::Get {
            target,
            database_id,
        } => Prepared::Get {
            target,
            database_id,
        },
        DatabaseCommand::Update {
            target,
            database_id,
            name,
            instruction,
            analysis_model,
            clear_analysis_model,
        } => {
            let request = UpdateDatabaseMemoryRequest {
                name,
                instruction: instruction.read()?,
                // The API's own convention: an empty id removes the binding.
                analysis_model_id: if clear_analysis_model {
                    Some(String::new())
                } else {
                    analysis_model
                },
            };
            if request.is_empty() {
                bail!(
                    "nothing to update: pass at least one of --name, --instruction, \
                     --instruction-file, --analysis-model, --clear-analysis-model"
                );
            }
            Prepared::Update {
                target,
                database_id,
                request,
            }
        }
        DatabaseCommand::Delete {
            target,
            database_id,
        } => Prepared::Delete {
            target,
            database_id,
        },
        DatabaseCommand::Reload {
            target,
            database_id,
        } => Prepared::Reload {
            target,
            database_id,
        },
        DatabaseCommand::GenerateInstruction {
            target,
            database_id,
        } => Prepared::GenerateInstruction {
            target,
            database_id,
        },
    })
}

fn execute(
    client: &Client,
    workspace_of: impl Fn(Option<String>) -> Result<String>,
    command: Prepared,
) -> Result<()> {
    match command {
        Prepared::List { target, params } => {
            let workspace = workspace_of(target.workspace)?;
            let project = target.project;
            let data = list_database_memories(client, &workspace, &project, &params)
                .with_context(|| format!("list database memories in project `{project}`"))?;
            print_json(&data)
        }
        Prepared::Create { target, request } => {
            let workspace = workspace_of(target.workspace)?;
            let project = target.project;
            let data = create_database_memory(client, &workspace, &project, &request)
                .with_context(|| format!("create database memory in project `{project}`"))?;
            print_json(&data)
        }
        Prepared::Get {
            target,
            database_id,
        } => {
            let workspace = workspace_of(target.workspace)?;
            let data = get_database_memory(client, &workspace, &target.project, &database_id)
                .with_context(|| format!("get database memory `{database_id}`"))?;
            print_json(&data)
        }
        Prepared::Update {
            target,
            database_id,
            request,
        } => {
            let workspace = workspace_of(target.workspace)?;
            let data =
                update_database_memory(client, &workspace, &target.project, &database_id, &request)
                    .with_context(|| format!("update database memory `{database_id}`"))?;
            print_json(&data)
        }
        Prepared::Delete {
            target,
            database_id,
        } => {
            let workspace = workspace_of(target.workspace)?;
            let project = target.project;
            delete_database_memory(client, &workspace, &project, &database_id)
                .with_context(|| format!("delete database memory `{database_id}`"))?;
            println!("Deleted database memory `{database_id}` from project `{project}`");
            Ok(())
        }
        Prepared::Reload {
            target,
            database_id,
        } => {
            let workspace = workspace_of(target.workspace)?;
            reload_database_memory(client, &workspace, &target.project, &database_id)
                .with_context(|| format!("reload database memory `{database_id}`"))?;
            println!("Started a schema reload for database memory `{database_id}`");
            Ok(())
        }
        Prepared::GenerateInstruction {
            target,
            database_id,
        } => {
            let workspace = workspace_of(target.workspace)?;
            let data =
                generate_database_instruction(client, &workspace, &target.project, &database_id)
                    .with_context(|| {
                        format!("generate an instruction for database memory `{database_id}`")
                    })?;
            print_json(&data)
        }
    }
}
