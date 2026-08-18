//! `memorylake fact` commands.
//!
//! Facts are single remembered statements, each owned by exactly one scope —
//! an actor or a project. `add` and `delete` therefore take the scope as a
//! required, mutually exclusive `--actor` / `--project` pair, mirroring the
//! two endpoint shapes; `list` reads across the workspace and filters by
//! owning scope instead.

use anyhow::{Context, Result, bail};
use clap::Subcommand;
use memorylake_core::api::facts::{
    AddFactsRequest, FactScope, ListFactsParams, add_facts, forget_fact, list_facts,
};
use memorylake_core::{Client, Paths, ResolveOverrides, resolve};

use super::require_workspace;
use super::search::{IdList, parse_id_list};

/// `fact` subcommands.
#[derive(Debug, Subcommand)]
pub enum FactCommand {
    /// Store facts in one scope.
    ///
    /// Facts are stored verbatim and are searchable immediately. Facts are
    /// immutable — to update one, simply add the new statement; the server
    /// resolves semantic conflicts between facts itself.
    Add {
        /// Workspace id the scope belongs to.
        ///
        /// Defaults to the workspace remembered by `workspace use`.
        #[arg(long)]
        workspace: Option<String>,
        /// Store as this actor's facts. Exactly one of --actor / --project.
        #[arg(long)]
        actor: Option<String>,
        /// Store as this project's facts. Exactly one of --actor / --project.
        #[arg(long)]
        project: Option<String>,
        /// Fact texts to store, one atomic statement each.
        #[arg(required = true, value_name = "TEXT")]
        facts: Vec<String>,
    },
    /// Delete facts by id, one scope at a time.
    ///
    /// Each id is deleted with its own request (the API's `forget` endpoint)
    /// and reported individually. An id that does not exist in the scope
    /// lands in `not_found` instead of failing the others — facts live in
    /// exactly one scope, so a wrong-scope id is an expected outcome, not an
    /// error.
    Delete {
        /// Workspace id the scope belongs to.
        ///
        /// Defaults to the workspace remembered by `workspace use`.
        #[arg(long)]
        workspace: Option<String>,
        /// Delete from this actor's facts. Exactly one of --actor / --project.
        #[arg(long)]
        actor: Option<String>,
        /// Delete from this project's facts. Exactly one of --actor / --project.
        #[arg(long)]
        project: Option<String>,
        /// Fact ids to delete.
        #[arg(required = true, value_name = "FACT_ID")]
        fact_ids: Vec<String>,
    },
    /// List facts across a workspace, filtered by owning scope.
    List {
        /// Workspace id to list in.
        ///
        /// Defaults to the workspace remembered by `workspace use`.
        #[arg(long)]
        workspace: Option<String>,
        /// Limit to facts owned by these actors (comma-separated).
        #[arg(long, value_name = "IDS", value_parser = parse_id_list)]
        actors: Option<IdList>,
        /// Limit to facts owned by these projects (comma-separated).
        #[arg(long, value_name = "IDS", value_parser = parse_id_list)]
        projects: Option<IdList>,
        /// Page size. The server caps this at 50.
        #[arg(long)]
        page_size: Option<u32>,
        /// Continuation token from a previous page.
        #[arg(long)]
        continuation_token: Option<String>,
    },
}

/// Resolve the required, mutually exclusive `--actor` / `--project` pair.
///
/// Enforced at runtime rather than through a clap group so the error can spell
/// out the scope model instead of a generic conflict message.
fn resolve_scope(actor: Option<String>, project: Option<String>) -> Result<FactScope> {
    match (actor, project) {
        (Some(actor_id), None) => Ok(FactScope::Actor(actor_id)),
        (None, Some(project_id)) => Ok(FactScope::Project(project_id)),
        (Some(_), Some(_)) => {
            bail!("a fact belongs to exactly one scope; pass --actor or --project, not both")
        }
        (None, None) => {
            bail!(
                "a scope is required: --actor <id> for an actor's facts, --project <id> for a project's"
            )
        }
    }
}

/// Execute a `fact` subcommand.
pub fn run(command: FactCommand, profile: Option<String>, base_url: Option<String>) -> Result<()> {
    let paths = Paths::default_home().context("resolve MemoryLake config paths")?;
    let runtime = resolve(&paths, &ResolveOverrides { profile, base_url })
        .context("resolve API credentials")?;
    let client = Client::new(&runtime.base_url, &runtime.api_key).context("build API client")?;

    match command {
        FactCommand::Add {
            workspace,
            actor,
            project,
            facts,
        } => {
            let workspace = require_workspace(&paths, &runtime.profile, workspace)?;
            let scope = resolve_scope(actor, project)?;
            let request = AddFactsRequest { facts };
            let data = add_facts(&client, &workspace, &scope, &request).context("add facts")?;
            println!("{}", serde_json::to_string_pretty(&data)?);
        }
        FactCommand::Delete {
            workspace,
            actor,
            project,
            fact_ids,
        } => {
            let workspace = require_workspace(&paths, &runtime.profile, workspace)?;
            let scope = resolve_scope(actor, project)?;
            let mut forgotten = Vec::new();
            let mut not_found = Vec::new();
            for fact_id in fact_ids {
                let existed = forget_fact(&client, &workspace, &scope, &fact_id)
                    .with_context(|| format!("delete fact `{fact_id}`"))?;
                if existed {
                    forgotten.push(fact_id);
                } else {
                    not_found.push(fact_id);
                }
            }
            let outcome = serde_json::json!({
                "forgotten": forgotten,
                "not_found": not_found,
            });
            // Printed before any failure is raised, like `project document
            // import`: the deletions that succeeded have already happened,
            // and the caller must not have to choose between seeing them and
            // seeing the failure.
            println!("{}", serde_json::to_string_pretty(&outcome)?);
            if !not_found.is_empty() {
                bail!(
                    "{} fact id(s) were not found in the given scope: {}",
                    not_found.len(),
                    not_found.join(", ")
                );
            }
        }
        FactCommand::List {
            workspace,
            actors,
            projects,
            page_size,
            continuation_token,
        } => {
            let workspace = require_workspace(&paths, &runtime.profile, workspace)?;
            if actors.is_none() && projects.is_none() {
                // The endpoint answers an empty page in that case (measured
                // 2026-08-07), which would read as "no facts exist" — reject
                // the request instead of relaying a misleading answer.
                bail!(
                    "at least one of --actors / --projects is required; \
                     the API returns nothing when neither filter is given"
                );
            }
            let params = ListFactsParams {
                actor_ids: actors.map(|list| list.0).unwrap_or_default(),
                project_ids: projects.map(|list| list.0).unwrap_or_default(),
                page_size,
                continuation_token,
            };
            let data = list_facts(&client, &workspace, &params).context("list facts")?;
            println!("{}", serde_json::to_string_pretty(&data)?);
        }
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn an_actor_scope_resolves_alone() {
        assert_eq!(
            resolve_scope(Some("actor-1".into()), None).expect("actor scope"),
            FactScope::Actor("actor-1".into())
        );
    }

    #[test]
    fn a_project_scope_resolves_alone() {
        assert_eq!(
            resolve_scope(None, Some("proj-1".into())).expect("project scope"),
            FactScope::Project("proj-1".into())
        );
    }

    #[test]
    fn both_scopes_are_rejected() {
        let err = resolve_scope(Some("actor-1".into()), Some("proj-1".into()))
            .expect_err("both must be rejected");
        assert!(err.to_string().contains("not both"), "{err}");
    }

    #[test]
    fn a_missing_scope_is_rejected_with_both_options_named() {
        let err = resolve_scope(None, None).expect_err("missing scope must be rejected");
        let message = err.to_string();
        assert!(message.contains("--actor"), "{message}");
        assert!(message.contains("--project"), "{message}");
    }
}
