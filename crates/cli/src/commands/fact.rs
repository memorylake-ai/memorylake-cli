//! `memorylake fact` commands.
//!
//! Facts are single remembered statements, each owned by exactly one scope —
//! an actor, a project, or an agent. Every command that addresses one scope
//! takes it as a required, mutually exclusive `--actor` / `--project` /
//! `--agent` trio, mirroring the endpoint shapes; `list` reads across the
//! workspace and filters by owning scope instead. Each scope's memory
//! conflicts and fact instruction live under `fact conflict` and
//! `fact instruction`.
//!
//! Every local check (scope flags, ranges, JSON shape) runs before
//! credentials are resolved, so a malformed command fails the same way
//! whether or not the caller is logged in.

mod conflict;
mod instruction;
mod scope;

use std::collections::BTreeSet;

use anyhow::{Context, Result, bail};
use clap::Subcommand;
use memorylake_core::api::facts::{
    AddFactsRequest, ListFactsParams, MAX_FACT_LIST_OWNERS, MAX_FACT_PAGE_SIZE, UpdateFactRequest,
    add_facts, forget_fact, get_fact, list_facts, trace_fact, update_fact,
};
use serde_json::{Map, Value};

use super::actor::parse_metadata_object;
use super::print_json;
use super::search::{IdList, parse_id_list};
use conflict::ConflictCommand;
use instruction::InstructionCommand;
use scope::{ScopeArgs, Session};

/// `fact` subcommands.
#[derive(Debug, Subcommand)]
pub enum FactCommand {
    /// Store facts in one scope.
    ///
    /// Facts are stored verbatim and are searchable immediately. To change an
    /// existing fact use `fact update`; the server also detects facts that
    /// contradict each other (see `fact conflict`).
    Add {
        #[command(flatten)]
        scope: ScopeArgs,
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
    /// error. A forgotten fact's history stays readable with `fact trace`.
    Delete {
        #[command(flatten)]
        scope: ScopeArgs,
        /// Fact ids to delete.
        #[arg(required = true, value_name = "FACT_ID")]
        fact_ids: Vec<String>,
    },
    /// Show one fact.
    ///
    /// The id must belong to the given scope. A forgotten fact is not found
    /// here; `fact trace` still shows it.
    Get {
        #[command(flatten)]
        scope: ScopeArgs,
        /// Fact id.
        #[arg(value_name = "FACT_ID")]
        fact_id: String,
    },
    /// Edit a fact's text and/or metadata in place.
    ///
    /// Fields left out are unchanged. `--metadata` replaces the whole stored
    /// metadata object rather than merging into it; pass `'{}'` to clear it.
    /// A text change is recorded in the fact's history (`fact trace`).
    Update {
        #[command(flatten)]
        scope: ScopeArgs,
        /// Fact id.
        #[arg(value_name = "FACT_ID")]
        fact_id: String,
        /// New fact text. Must not be blank.
        #[arg(long)]
        text: Option<String>,
        /// New metadata as a JSON object, replacing the stored one whole.
        #[arg(long, value_parser = parse_metadata_object)]
        metadata: Option<Map<String, Value>>,
    },
    /// Show a fact and its full change history, newest first.
    ///
    /// Works for forgotten facts too. Each entry says what changed (`ADD`,
    /// `UPDATE`, `FORGET`) and how (`COOK`: extracted from a conversation, with
    /// its `conversation_id` and message ids; `MANUAL`: through the API).
    Trace {
        #[command(flatten)]
        scope: ScopeArgs,
        /// Fact id.
        #[arg(value_name = "FACT_ID")]
        fact_id: String,
    },
    /// List facts across a workspace, filtered by owning scope, newest first.
    ///
    /// At least one of --actors / --projects / --agents is required, naming
    /// at most 50 distinct owners in total. Each fact carries its `owner`.
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
        /// Limit to facts owned by these agents (comma-separated).
        #[arg(long, value_name = "IDS", value_parser = parse_id_list)]
        agents: Option<IdList>,
        /// Keep only facts whose text contains this substring.
        #[arg(long, value_name = "TEXT", value_parser = parse_query)]
        query: Option<String>,
        /// Page size, 1-200. The server defaults to 50.
        #[arg(long, value_parser = clap::value_parser!(u32).range(1..=i64::from(MAX_FACT_PAGE_SIZE)))]
        page_size: Option<u32>,
        /// Continuation token from a previous page.
        #[arg(long)]
        continuation_token: Option<String>,
    },
    /// Review and resolve contradictions the server found between facts.
    Conflict {
        #[command(subcommand)]
        command: ConflictCommand,
    },
    /// Read, set, or draft the instruction that steers what a scope records.
    Instruction {
        #[command(subcommand)]
        command: InstructionCommand,
    },
}

/// Reject an empty `--query`, which would read as "no filter" server-side.
fn parse_query(raw: &str) -> std::result::Result<String, String> {
    if raw.is_empty() {
        return Err("must not be empty".to_string());
    }
    Ok(raw.to_string())
}

/// Build the `fact update` body, rejecting an edit that changes nothing.
///
/// Blank text is rejected rather than sent: the server treats it as absent
/// (measured 2026-10-09), so `--text ' ' --metadata ...` would silently update
/// only the metadata.
fn build_update(
    text: Option<String>,
    metadata: Option<Map<String, Value>>,
) -> Result<UpdateFactRequest> {
    if text.as_deref().is_some_and(|text| text.trim().is_empty()) {
        bail!("--text must not be blank; to remove a fact, use `fact delete`");
    }
    if text.is_none() && metadata.is_none() {
        bail!("nothing to update: pass --text and/or --metadata");
    }
    Ok(UpdateFactRequest {
        fact: text,
        metadata,
    })
}

/// Collect the `fact list` owner filters, enforcing the API's limits.
fn build_list_owners(
    actors: Option<IdList>,
    projects: Option<IdList>,
    agents: Option<IdList>,
) -> Result<(Vec<String>, Vec<String>, Vec<String>)> {
    let actors = actors.map(|list| list.0).unwrap_or_default();
    let projects = projects.map(|list| list.0).unwrap_or_default();
    let agents = agents.map(|list| list.0).unwrap_or_default();
    if actors.is_empty() && projects.is_empty() && agents.is_empty() {
        // The endpoint rejects an owner-less listing (and once answered it
        // with an empty page), so fail before sending it.
        bail!("at least one of --actors / --projects / --agents is required");
    }
    let distinct: BTreeSet<&String> = actors.iter().chain(&projects).chain(&agents).collect();
    if distinct.len() > MAX_FACT_LIST_OWNERS {
        bail!(
            "--actors / --projects / --agents name {} distinct owners; the API accepts at most {MAX_FACT_LIST_OWNERS}",
            distinct.len()
        );
    }
    Ok((actors, projects, agents))
}

/// Execute a `fact` subcommand.
pub fn run(command: FactCommand, profile: Option<String>, base_url: Option<String>) -> Result<()> {
    match command {
        FactCommand::Add { scope, facts } => {
            let (workspace, scope) = scope.resolve()?;
            let session = Session::open(profile, base_url)?;
            let workspace = session.workspace(workspace)?;
            let request = AddFactsRequest { facts };
            let data =
                add_facts(&session.client, &workspace, &scope, &request).context("add facts")?;
            print_json(&data)?;
        }
        FactCommand::Delete { scope, fact_ids } => {
            let (workspace, scope) = scope.resolve()?;
            let session = Session::open(profile, base_url)?;
            let workspace = session.workspace(workspace)?;
            let mut forgotten = Vec::new();
            let mut not_found = Vec::new();
            for fact_id in fact_ids {
                let existed = forget_fact(&session.client, &workspace, &scope, &fact_id)
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
            print_json(&outcome)?;
            if !not_found.is_empty() {
                bail!(
                    "{} fact id(s) were not found in the given scope: {}",
                    not_found.len(),
                    not_found.join(", ")
                );
            }
        }
        FactCommand::Get { scope, fact_id } => {
            let (workspace, scope) = scope.resolve()?;
            let session = Session::open(profile, base_url)?;
            let workspace = session.workspace(workspace)?;
            let data = get_fact(&session.client, &workspace, &scope, &fact_id)
                .with_context(|| format!("get fact `{fact_id}`"))?;
            print_json(&data)?;
        }
        FactCommand::Update {
            scope,
            fact_id,
            text,
            metadata,
        } => {
            let (workspace, scope) = scope.resolve()?;
            let request = build_update(text, metadata)?;
            let session = Session::open(profile, base_url)?;
            let workspace = session.workspace(workspace)?;
            let data = update_fact(&session.client, &workspace, &scope, &fact_id, &request)
                .with_context(|| format!("update fact `{fact_id}`"))?;
            print_json(&data)?;
        }
        FactCommand::Trace { scope, fact_id } => {
            let (workspace, scope) = scope.resolve()?;
            let session = Session::open(profile, base_url)?;
            let workspace = session.workspace(workspace)?;
            let data = trace_fact(&session.client, &workspace, &scope, &fact_id)
                .with_context(|| format!("trace fact `{fact_id}`"))?;
            print_json(&data)?;
        }
        FactCommand::List {
            workspace,
            actors,
            projects,
            agents,
            query,
            page_size,
            continuation_token,
        } => {
            let (actor_ids, project_ids, agent_ids) = build_list_owners(actors, projects, agents)?;
            let session = Session::open(profile, base_url)?;
            let workspace = session.workspace(workspace)?;
            let params = ListFactsParams {
                actor_ids,
                project_ids,
                agent_ids,
                fact_fuzzy: query,
                page_size,
                continuation_token,
            };
            let data = list_facts(&session.client, &workspace, &params).context("list facts")?;
            print_json(&data)?;
        }
        FactCommand::Conflict { command } => conflict::run(command, profile, base_url)?,
        FactCommand::Instruction { command } => instruction::run(command, profile, base_url)?,
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn ids(raw: &[&str]) -> Option<IdList> {
        Some(IdList(raw.iter().map(|id| id.to_string()).collect()))
    }

    #[test]
    fn an_update_needs_text_or_metadata() {
        let err = build_update(None, None).expect_err("empty update");
        assert!(err.to_string().contains("nothing to update"), "{err}");
    }

    #[test]
    fn blank_update_text_is_rejected_even_with_metadata() {
        let err = build_update(Some("  ".into()), Some(Map::new())).expect_err("blank text");
        assert!(err.to_string().contains("must not be blank"), "{err}");
    }

    #[test]
    fn metadata_alone_is_a_valid_update() {
        let request = build_update(None, Some(Map::new())).expect("metadata-only");
        assert_eq!(request.fact, None);
        assert_eq!(request.metadata, Some(Map::new()));
    }

    #[test]
    fn a_listing_needs_an_owner() {
        let err = build_list_owners(None, None, None).expect_err("no owner");
        assert!(err.to_string().contains("--agents"), "{err}");
    }

    #[test]
    fn owners_are_capped_by_distinct_count() {
        let many: Vec<String> = (0..51).map(|i| format!("actor-{i}")).collect();
        let refs: Vec<&str> = many.iter().map(String::as_str).collect();
        let err = build_list_owners(ids(&refs), None, None).expect_err("51 owners");
        assert!(err.to_string().contains("at most 50"), "{err}");

        // Repeats count once.
        let repeated = vec!["actor-1"; 60];
        build_list_owners(ids(&repeated), None, None).expect("one distinct owner");
    }

    #[test]
    fn every_owner_kind_is_passed_through() {
        let (actors, projects, agents) =
            build_list_owners(ids(&["a"]), ids(&["p"]), ids(&["g"])).expect("valid");
        assert_eq!(actors, vec!["a".to_string()]);
        assert_eq!(projects, vec!["p".to_string()]);
        assert_eq!(agents, vec!["g".to_string()]);
    }
}
