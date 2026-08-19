//! `memorylake search` command.

use super::require_workspace;
use anyhow::{Context, Result};
use clap::Args;
use memorylake_core::api::search::{MemoryType, SearchMemoriesRequest, search_memories};
use memorylake_core::{Client, Paths, ResolveOverrides, resolve};

/// Search memories in a workspace.
///
/// The filter flags each take one comma-separated value rather than being
/// repeated, and every list is validated before any request goes out.
#[derive(Debug, Args)]
pub struct SearchArgs {
    /// Natural language query.
    #[arg(value_parser = parse_query)]
    query: String,
    /// Workspace id to search in.
    ///
    /// Defaults to the workspace remembered by `workspace use`.
    #[arg(long)]
    workspace: Option<String>,
    /// Limit to these projects (comma-separated). Defaults to every project.
    #[arg(long, value_name = "IDS", value_parser = parse_id_list)]
    projects: Option<IdList>,
    /// Limit to memories associated with these actors (comma-separated).
    #[arg(long, value_name = "IDS", value_parser = parse_id_list)]
    actors: Option<IdList>,
    /// Limit to these memory types (comma-separated): document, fact.
    #[arg(long, value_name = "TYPES", value_parser = parse_memory_types)]
    types: Option<MemoryTypeList>,
    /// Maximum results per source type. The server picks a default when unset.
    #[arg(long)]
    top_k: Option<u32>,
}

/// Reject a query that carries no searchable text.
///
/// Trimming here also keeps a stray shell-quoted space out of the request.
fn parse_query(raw: &str) -> std::result::Result<String, String> {
    let query = raw.trim();
    if query.is_empty() {
        return Err("must not be empty".to_string());
    }
    Ok(query.to_string())
}

/// One flag's worth of ids, already split and validated.
///
/// The `Vec` is wrapped because clap's derive reads a bare `Option<Vec<T>>` as
/// a repeatable flag yielding one `T` per occurrence, which does not match a
/// parser that returns the whole list from a single value.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct IdList(pub(crate) Vec<String>);

/// One flag's worth of memory types, already split and validated.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MemoryTypeList(Vec<MemoryType>);

/// Split one comma-separated flag value into trimmed, non-empty entries.
///
/// An empty entry is rejected rather than dropped: `--projects a,,b` is far
/// more likely to be a typo than a request to search two projects, and
/// silently ignoring it would quietly change the caller's filter.
pub(crate) fn split_csv(raw: &str) -> std::result::Result<Vec<String>, String> {
    if raw.trim().is_empty() {
        return Err("must not be empty".to_string());
    }
    let mut entries = Vec::new();
    for segment in raw.split(',') {
        let trimmed = segment.trim();
        if trimmed.is_empty() {
            return Err(
                "must not contain an empty entry; check for a doubled or trailing comma"
                    .to_string(),
            );
        }
        entries.push(trimmed.to_string());
    }
    Ok(entries)
}

/// Parse a comma-separated id filter.
pub(crate) fn parse_id_list(raw: &str) -> std::result::Result<IdList, String> {
    split_csv(raw).map(IdList)
}

/// Parse a comma-separated memory-type filter, rejecting unknown values.
fn parse_memory_types(raw: &str) -> std::result::Result<MemoryTypeList, String> {
    split_csv(raw)?
        .into_iter()
        .map(|entry| {
            MemoryType::from_wire(&entry).ok_or_else(|| {
                format!(
                    "unknown memory type `{entry}`; expected one of {}",
                    accepted()
                )
            })
        })
        .collect::<std::result::Result<Vec<_>, _>>()
        .map(MemoryTypeList)
}

/// Accepted `--types` values, rendered for an error message.
fn accepted() -> String {
    MemoryType::ALL
        .into_iter()
        .map(MemoryType::as_str)
        .collect::<Vec<_>>()
        .join(", ")
}

/// Execute the `search` command.
pub fn run(args: SearchArgs, profile: Option<String>, base_url: Option<String>) -> Result<()> {
    let paths = Paths::default_home().context("resolve MemoryLake config paths")?;
    let runtime = resolve(&paths, &ResolveOverrides { profile, base_url })
        .context("resolve API credentials")?;
    let client = Client::new(&runtime.base_url, &runtime.api_key).context("build API client")?;

    let workspace = require_workspace(&paths, &runtime.profile, args.workspace)?;
    let request = SearchMemoriesRequest {
        query: args.query,
        project_ids: args.projects.map(|list| list.0),
        actor_ids: args.actors.map(|list| list.0),
        memory_types: args.types.map(|list| list.0),
        top_k: args.top_k,
    };
    let data = search_memories(&client, &workspace, &request).context("search memories")?;
    println!("{}", serde_json::to_string_pretty(&data)?);

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn ids(raw: &str) -> Vec<String> {
        parse_id_list(raw).expect("should parse").0
    }

    fn types(raw: &str) -> Vec<MemoryType> {
        parse_memory_types(raw).expect("should parse").0
    }

    #[test]
    fn parse_query_trims_surrounding_whitespace() {
        assert_eq!(
            parse_query("  quarterly revenue  ").unwrap(),
            "quarterly revenue"
        );
    }

    #[test]
    fn parse_query_rejects_empty_and_blank() {
        for raw in ["", "   ", "\t", "\n"] {
            assert_eq!(
                parse_query(raw).expect_err("blank query must be rejected"),
                "must not be empty"
            );
        }
    }

    #[test]
    fn id_list_splits_and_trims() {
        assert_eq!(ids("P1,P2"), vec!["P1", "P2"]);
        assert_eq!(ids("P1, P2"), vec!["P1", "P2"]);
        assert_eq!(ids("  P1  ,\tP2 "), vec!["P1", "P2"]);
    }

    #[test]
    fn id_list_accepts_a_single_entry() {
        assert_eq!(ids("P1"), vec!["P1"]);
        assert_eq!(ids("  P1  "), vec!["P1"]);
    }

    #[test]
    fn id_list_rejects_an_entirely_empty_value() {
        for raw in ["", "   "] {
            assert_eq!(
                parse_id_list(raw).expect_err("empty value must be rejected"),
                "must not be empty"
            );
        }
    }

    #[test]
    fn id_list_rejects_empty_entries() {
        for raw in ["P1,,P2", ",", "P1,", ",P1", "P1, ,P2", ",,"] {
            match parse_id_list(raw) {
                Ok(parsed) => panic!("{raw:?} should have been rejected, got {parsed:?}"),
                Err(err) => assert!(err.contains("empty entry"), "{raw:?}: {err}"),
            }
        }
    }

    #[test]
    fn memory_types_accept_the_documented_values() {
        assert_eq!(types("document"), vec![MemoryType::Document]);
        assert_eq!(types("fact"), vec![MemoryType::Fact]);
        assert_eq!(
            types("document, fact"),
            vec![MemoryType::Document, MemoryType::Fact]
        );
    }

    #[test]
    fn memory_types_reject_unknown_values_and_list_the_valid_ones() {
        for raw in ["Document", "DOCUMENT", "facts", "memo", "document,memo"] {
            let err = parse_memory_types(raw).expect_err("unknown type must be rejected");
            assert!(err.contains("unknown memory type"), "{raw}: {err}");
            assert!(err.contains("document, fact"), "{raw}: {err}");
        }
    }

    #[test]
    fn memory_types_report_the_offending_value() {
        let err = parse_memory_types("document,memo").expect_err("must reject");
        assert!(err.contains("`memo`"), "{err}");
    }

    #[test]
    fn memory_types_inherit_the_empty_entry_rule() {
        let err = parse_memory_types("document,,fact").expect_err("must reject");
        assert!(err.contains("empty entry"), "{err}");
    }
}
