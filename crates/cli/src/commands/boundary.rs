//! `memorylake boundary` commands.
//!
//! A boundary is a named, saved search scope inside a workspace: a set of
//! projects plus, optionally, one human actor and one agent whose memories are
//! in scope. Everything it names must belong to its workspace, and creating or
//! changing one needs search permission on each of them.

use anyhow::{Context, Result, bail};
use clap::Subcommand;
use memorylake_core::api::boundaries::{
    CreateBoundaryRequest, ListBoundariesParams, UpdateBoundaryRequest, create_boundary,
    delete_boundary, get_boundary, get_boundary_by_custom_id, list_boundaries, update_boundary,
};
use memorylake_core::{Client, Paths, ResolveOverrides, resolve};

use super::search::{IdList, parse_id_list};
use super::{parse_non_blank, print_json, require_workspace};

/// Longest boundary name the API accepts.
const MAX_NAME_CHARS: usize = 255;

/// Boundary subcommands.
#[derive(Debug, Subcommand)]
pub enum BoundaryCommand {
    /// List the boundaries in a workspace.
    List {
        /// Workspace whose boundaries to list.
        ///
        /// Defaults to the workspace remembered by `workspace use`.
        #[arg(long)]
        workspace: Option<String>,
        /// Number of items per page (1-100).
        #[arg(long, value_parser = clap::value_parser!(u32).range(1..=100))]
        page_size: Option<u32>,
        /// Continuation token from a previous response.
        #[arg(long)]
        continuation_token: Option<String>,
        /// Fuzzy filter by boundary name (partial match).
        #[arg(long = "name")]
        name_fuzzy: Option<String>,
    },
    /// Create a boundary.
    ///
    /// Without scope flags the boundary scopes nothing. Projects must exist in
    /// the workspace; the human actor and the agent must be bound to it.
    Create {
        /// Workspace the boundary belongs to. Cannot be changed later.
        ///
        /// Defaults to the workspace remembered by `workspace use`.
        #[arg(long)]
        workspace: Option<String>,
        /// Display name.
        #[arg(long, value_parser = parse_name)]
        name: String,
        /// Caller-defined id, unique within the workspace.
        #[arg(long, value_parser = parse_non_blank)]
        custom_id: Option<String>,
        /// Projects in scope (comma-separated ids).
        #[arg(long, value_name = "IDS", value_parser = parse_id_list)]
        projects: Option<IdList>,
        /// Human actor in scope.
        #[arg(long, value_name = "ID", value_parser = parse_id)]
        human_actor: Option<String>,
        /// Agent whose memories are in scope.
        #[arg(long, value_name = "ID", value_parser = parse_id)]
        agent: Option<String>,
    },
    /// Get a single boundary by id.
    Get {
        /// Boundary id (or custom_id when `--by-custom-id` is set).
        #[arg(value_parser = parse_non_blank)]
        id: String,
        /// Treat the positional argument as a caller-defined custom_id.
        #[arg(long)]
        by_custom_id: bool,
    },
    /// Update a boundary. Only the flags you pass are changed.
    Update {
        /// Boundary id.
        #[arg(value_parser = parse_non_blank)]
        id: String,
        /// New display name.
        #[arg(long, value_parser = parse_name)]
        name: Option<String>,
        /// New projects in scope (comma-separated ids). REPLACES the current
        /// list — name every project you want to keep.
        #[arg(long, value_name = "IDS", value_parser = parse_id_list, conflicts_with = "clear_projects")]
        projects: Option<IdList>,
        /// Remove project scoping.
        #[arg(long)]
        clear_projects: bool,
        /// New human actor in scope.
        #[arg(long, value_name = "ID", value_parser = parse_id, conflicts_with = "clear_human_actor")]
        human_actor: Option<String>,
        /// Remove the human actor from the scope.
        #[arg(long)]
        clear_human_actor: bool,
        /// New agent in scope.
        #[arg(long, value_name = "ID", value_parser = parse_id, conflicts_with = "clear_agent")]
        agent: Option<String>,
        /// Remove the agent from the scope.
        #[arg(long)]
        clear_agent: bool,
    },
    /// Delete a boundary. There is no confirmation prompt.
    Delete {
        /// Boundary id.
        #[arg(value_parser = parse_non_blank)]
        id: String,
    },
}

/// Execute a `boundary` subcommand.
pub fn run(
    command: BoundaryCommand,
    profile: Option<String>,
    base_url: Option<String>,
) -> Result<()> {
    // Decide the update body first: an empty one is a local mistake and must
    // not wait on credentials.
    let update = match &command {
        BoundaryCommand::Update {
            name,
            projects,
            clear_projects,
            human_actor,
            clear_human_actor,
            agent,
            clear_agent,
            ..
        } => {
            let request = UpdateBoundaryRequest {
                name: name.clone(),
                project_ids: cleared_list(projects, *clear_projects),
                human_actor_id: cleared_id(human_actor, *clear_human_actor),
                agent_id: cleared_id(agent, *clear_agent),
            };
            if request.is_empty() {
                bail!(
                    "nothing to update; pass --name, --projects/--clear-projects, \
                     --human-actor/--clear-human-actor, or --agent/--clear-agent"
                );
            }
            Some(request)
        }
        _ => None,
    };

    let paths = Paths::default_home().context("resolve MemoryLake config paths")?;
    let runtime = resolve(&paths, &ResolveOverrides { profile, base_url })
        .context("resolve API credentials")?;
    let client = Client::new(&runtime.base_url, &runtime.api_key).context("build API client")?;

    match command {
        BoundaryCommand::List {
            workspace,
            page_size,
            continuation_token,
            name_fuzzy,
        } => {
            let workspace = require_workspace(&paths, &runtime.profile, workspace)?;
            let data = list_boundaries(
                &client,
                &workspace,
                &ListBoundariesParams {
                    page_size,
                    continuation_token,
                    name_fuzzy,
                },
            )
            .with_context(|| format!("list boundaries in workspace `{workspace}`"))?;
            print_json(&data)?;
        }
        BoundaryCommand::Create {
            workspace,
            name,
            custom_id,
            projects,
            human_actor,
            agent,
        } => {
            let workspace = require_workspace(&paths, &runtime.profile, workspace)?;
            let data = create_boundary(
                &client,
                &CreateBoundaryRequest {
                    name,
                    workspace_id: workspace.clone(),
                    custom_id,
                    project_ids: projects.map(|list| list.0),
                    human_actor_id: human_actor,
                    agent_id: agent,
                },
            )
            .with_context(|| format!("create boundary in workspace `{workspace}`"))?;
            print_json(&data)?;
        }
        BoundaryCommand::Get { id, by_custom_id } => {
            let data = if by_custom_id {
                get_boundary_by_custom_id(&client, &id)
                    .with_context(|| format!("get boundary by custom_id `{id}`"))?
            } else {
                get_boundary(&client, &id).with_context(|| format!("get boundary `{id}`"))?
            };
            print_json(&data)?;
        }
        BoundaryCommand::Update { id, .. } => {
            let request = update.context("update body is built before credentials")?;
            let data = update_boundary(&client, &id, &request)
                .with_context(|| format!("update boundary `{id}`"))?;
            print_json(&data)?;
        }
        BoundaryCommand::Delete { id } => {
            delete_boundary(&client, &id).with_context(|| format!("delete boundary `{id}`"))?;
            println!("Deleted boundary `{id}`");
        }
    }

    Ok(())
}

/// `--projects` or `--clear-projects` as the API spells them: an empty list
/// clears.
fn cleared_list(list: &Option<IdList>, clear: bool) -> Option<Vec<String>> {
    if clear {
        return Some(Vec::new());
    }
    list.as_ref().map(|list| list.0.clone())
}

/// An id flag or its `--clear-*` twin as the API spells them: an empty string
/// clears.
fn cleared_id(id: &Option<String>, clear: bool) -> Option<String> {
    if clear {
        return Some(String::new());
    }
    id.clone()
}

/// Accept a non-blank id, trimmed.
///
/// An empty value would silently mean "clear" to the API; that has its own
/// explicit `--clear-*` flag instead.
fn parse_id(raw: &str) -> std::result::Result<String, String> {
    let id = raw.trim();
    if id.is_empty() {
        return Err("must not be empty; use the matching --clear-* flag to remove it".to_string());
    }
    Ok(id.to_string())
}

/// Accept a non-blank name of at most 255 characters, unchanged.
fn parse_name(raw: &str) -> std::result::Result<String, String> {
    if raw.trim().is_empty() {
        return Err("must not be empty".to_string());
    }
    let chars = raw.chars().count();
    if chars > MAX_NAME_CHARS {
        return Err(format!(
            "must be at most {MAX_NAME_CHARS} characters, got {chars}"
        ));
    }
    Ok(raw.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn clear_flags_map_to_the_api_spelling() {
        assert_eq!(cleared_list(&None, true), Some(Vec::new()));
        assert_eq!(cleared_id(&None, true), Some(String::new()));
        assert_eq!(cleared_list(&None, false), None);
        assert_eq!(cleared_id(&Some("a".into()), false), Some("a".into()));
        assert_eq!(
            cleared_list(&Some(IdList(vec!["p".into()])), false),
            Some(vec!["p".to_string()])
        );
    }

    #[test]
    fn ids_must_not_be_blank() {
        assert!(parse_id(" ").unwrap_err().contains("--clear-"));
        assert_eq!(parse_id(" agent-1 ").unwrap(), "agent-1");
    }

    #[test]
    fn names_are_bounded() {
        assert!(parse_name("").is_err());
        assert!(parse_name(&"x".repeat(MAX_NAME_CHARS + 1)).is_err());
        assert_eq!(parse_name(" a ").unwrap(), " a ");
    }
}
