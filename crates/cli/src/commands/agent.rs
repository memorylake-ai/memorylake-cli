//! `memorylake agent` commands.
//!
//! Agent *identity* (name, description, metadata) changes in place via
//! `agent update`. Agent *configuration* (model, policies, prompt, …) is
//! immutable and changes only by creating a new version.

mod body;

use anyhow::{Context, Result};
use clap::Subcommand;
use memorylake_core::api::agents::{
    BindAgentRequest, ListAgentVersionsParams, ListAgentsParams, bind_agent, create_agent,
    create_agent_version, delete_agent, get_agent, get_agent_by_custom_id, get_agent_version,
    list_agent_versions, list_agents, list_workspace_agents, unbind_agent, update_agent,
};
use memorylake_core::{Client, Paths, ResolveOverrides, resolve};
use std::path::PathBuf;

use body::{FromVersion, load_config_body, reject_config_fields, require_field, set_scalar};

/// Agent subcommands.
#[derive(Debug, Subcommand)]
pub enum AgentCommand {
    /// List agents.
    List {
        /// Number of items per page.
        #[arg(long)]
        page_size: Option<u32>,
        /// Continuation token from a previous response.
        #[arg(long)]
        continuation_token: Option<String>,
        /// Fuzzy filter by agent name (partial match).
        #[arg(long = "name")]
        name_fuzzy: Option<String>,
    },
    /// Create an agent.
    ///
    /// Nested configuration (`policies`, `output`, `subagents`, `skills`,
    /// `metadata`, `capabilities`, `model_settings`, `runtime_bindings`) is
    /// supplied through `--config`. Scalar flags override same-named keys in
    /// that file, and unrecognized keys are forwarded to the API unchanged.
    Create {
        /// Agent display name.
        #[arg(long)]
        name: Option<String>,
        /// Caller-defined unique external id.
        #[arg(long)]
        custom_id: Option<String>,
        /// Optional description.
        #[arg(long)]
        description: Option<String>,
        /// Model identifier (e.g. `claude-sonnet-4-20250514`).
        #[arg(long)]
        model: Option<String>,
        /// Agent instruction prompt.
        #[arg(long)]
        system_prompt: Option<String>,
        /// JSON file holding the full request body.
        #[arg(long, value_name = "FILE")]
        config: Option<PathBuf>,
    },
    /// Get a single agent by id.
    Get {
        /// Agent id (or custom_id when `--by-custom-id` is set).
        id: String,
        /// Treat the positional argument as a caller-defined custom_id.
        #[arg(long)]
        by_custom_id: bool,
    },
    /// Update an agent's identity fields.
    ///
    /// Accepts `name`, `description`, and `metadata` only; `metadata` replaces
    /// the stored object outright. Configuration changes require
    /// `agent version create`.
    Update {
        /// Agent id.
        id: String,
        /// New display name.
        #[arg(long)]
        name: Option<String>,
        /// New description.
        #[arg(long)]
        description: Option<String>,
        /// JSON file holding identity fields (use this to set `metadata`).
        #[arg(long, value_name = "FILE")]
        config: Option<PathBuf>,
    },
    /// Delete an agent.
    ///
    /// Irreversible: removes the agent, every one of its versions, and all of
    /// its workspace bindings. There is no confirmation prompt.
    Delete {
        /// Agent id.
        id: String,
    },
    /// Manage agent configuration versions.
    Version {
        #[command(subcommand)]
        command: VersionCommand,
    },
    /// Bind an agent to a workspace.
    Bind {
        /// Agent id to bind.
        agent_id: String,
        /// Workspace to bind the agent into.
        #[arg(long)]
        workspace: String,
    },
    /// Unbind an agent from a workspace.
    ///
    /// Removes the binding only; the agent definition is left intact.
    Unbind {
        /// Agent id to unbind.
        agent_id: String,
        /// Workspace to remove the agent from.
        #[arg(long)]
        workspace: String,
    },
    /// List the agents bound to a workspace.
    Bindings {
        /// Workspace whose bindings to list.
        #[arg(long)]
        workspace: String,
        /// Number of items per page.
        #[arg(long)]
        page_size: Option<u32>,
        /// Continuation token from a previous response.
        #[arg(long)]
        continuation_token: Option<String>,
        /// Fuzzy filter by agent name (partial match).
        #[arg(long = "name")]
        name_fuzzy: Option<String>,
    },
}

/// `agent version` subcommands.
#[derive(Debug, Subcommand)]
pub enum VersionCommand {
    /// Create a new configuration version.
    Create {
        /// Agent id.
        id: String,
        /// Model identifier.
        #[arg(long)]
        model: Option<String>,
        /// Agent instruction prompt.
        #[arg(long)]
        system_prompt: Option<String>,
        /// JSON file holding the version configuration.
        #[arg(long, value_name = "FILE")]
        config: Option<PathBuf>,
        /// Start from an existing version and apply overrides on top.
        ///
        /// Overrides replace whole top-level keys; nested objects are not
        /// merged. Without this flag only the values you supply are sent.
        #[arg(long, value_name = "latest|N")]
        from_version: Option<FromVersion>,
    },
    /// List an agent's versions.
    List {
        /// Agent id.
        id: String,
        /// Number of items per page.
        #[arg(long)]
        page_size: Option<u32>,
        /// Continuation token from a previous response.
        #[arg(long)]
        continuation_token: Option<String>,
    },
    /// Get one version of an agent.
    Get {
        /// Agent id.
        id: String,
        /// Version number.
        version: u64,
    },
}

/// Execute an `agent` subcommand.
pub fn run(command: AgentCommand, profile: Option<String>, base_url: Option<String>) -> Result<()> {
    let paths = Paths::default_home().context("resolve MemoryLake config paths")?;
    let runtime = resolve(&paths, &ResolveOverrides { profile, base_url })
        .context("resolve API credentials")?;
    let client = Client::new(&runtime.base_url, &runtime.api_key).context("build API client")?;

    match command {
        AgentCommand::List {
            page_size,
            continuation_token,
            name_fuzzy,
        } => {
            let data = list_agents(
                &client,
                &ListAgentsParams {
                    page_size,
                    continuation_token,
                    name_fuzzy,
                },
            )
            .context("list agents")?;
            println!("{}", serde_json::to_string_pretty(&data)?);
        }
        AgentCommand::Create {
            name,
            custom_id,
            description,
            model,
            system_prompt,
            config,
        } => {
            let mut body = load_config_body(config.as_deref())?;
            set_scalar(&mut body, "name", name);
            set_scalar(&mut body, "custom_id", custom_id);
            set_scalar(&mut body, "description", description);
            set_scalar(&mut body, "model", model);
            set_scalar(&mut body, "system_prompt", system_prompt);
            require_field(&body, "name", "name")?;
            require_field(&body, "custom_id", "custom-id")?;

            let data = create_agent(&client, &body).context("create agent")?;
            println!("{}", serde_json::to_string_pretty(&data)?);
        }
        AgentCommand::Get { id, by_custom_id } => {
            let data = if by_custom_id {
                get_agent_by_custom_id(&client, &id).context("get agent by custom_id")?
            } else {
                get_agent(&client, &id).context("get agent")?
            };
            println!("{}", serde_json::to_string_pretty(&data)?);
        }
        AgentCommand::Update {
            id,
            name,
            description,
            config,
        } => {
            let mut body = load_config_body(config.as_deref())?;
            set_scalar(&mut body, "name", name);
            set_scalar(&mut body, "description", description);
            reject_config_fields(&body)?;
            if body.is_empty() {
                anyhow::bail!(
                    "nothing to update; pass --name, --description, or a --config file setting `metadata`"
                );
            }

            let data = update_agent(&client, &id, &body).context("update agent")?;
            println!("{}", serde_json::to_string_pretty(&data)?);
        }
        AgentCommand::Delete { id } => {
            delete_agent(&client, &id).context("delete agent")?;
            println!("Deleted agent `{id}` and all of its versions and bindings");
        }
        AgentCommand::Version { command } => run_version(&client, command)?,
        AgentCommand::Bind {
            agent_id,
            workspace,
        } => {
            let data = bind_agent(
                &client,
                &workspace,
                &BindAgentRequest {
                    agent_id: agent_id.clone(),
                },
            )
            .with_context(|| format!("bind agent `{agent_id}` to workspace `{workspace}`"))?;
            println!("{}", serde_json::to_string_pretty(&data)?);
        }
        AgentCommand::Unbind {
            agent_id,
            workspace,
        } => {
            unbind_agent(&client, &workspace, &agent_id).with_context(|| {
                format!("unbind agent `{agent_id}` from workspace `{workspace}`")
            })?;
            println!("Unbound agent `{agent_id}` from workspace `{workspace}`");
        }
        AgentCommand::Bindings {
            workspace,
            page_size,
            continuation_token,
            name_fuzzy,
        } => {
            let data = list_workspace_agents(
                &client,
                &workspace,
                &ListAgentsParams {
                    page_size,
                    continuation_token,
                    name_fuzzy,
                },
            )
            .with_context(|| format!("list agents bound to workspace `{workspace}`"))?;
            println!("{}", serde_json::to_string_pretty(&data)?);
        }
    }

    Ok(())
}

fn run_version(client: &Client, command: VersionCommand) -> Result<()> {
    match command {
        VersionCommand::Create {
            id,
            model,
            system_prompt,
            config,
            from_version,
        } => {
            let mut body = match &from_version {
                Some(from) => body::version_base_body(client, &id, from)?,
                None => Default::default(),
            };
            // Top-level key replacement: `--config` first, then scalar flags.
            for (key, value) in load_config_body(config.as_deref())? {
                body.insert(key, value);
            }
            set_scalar(&mut body, "model", model);
            set_scalar(&mut body, "system_prompt", system_prompt);

            let data = create_agent_version(client, &id, &body)
                .with_context(|| format!("create version for agent `{id}`"))?;
            println!("{}", serde_json::to_string_pretty(&data)?);
        }
        VersionCommand::List {
            id,
            page_size,
            continuation_token,
        } => {
            let data = list_agent_versions(
                client,
                &id,
                &ListAgentVersionsParams {
                    page_size,
                    continuation_token,
                },
            )
            .with_context(|| format!("list versions of agent `{id}`"))?;
            println!("{}", serde_json::to_string_pretty(&data)?);
        }
        VersionCommand::Get { id, version } => {
            let data = get_agent_version(client, &id, version)
                .with_context(|| format!("get version {version} of agent `{id}`"))?;
            println!("{}", serde_json::to_string_pretty(&data)?);
        }
    }

    Ok(())
}
