//! List workspace facts
//! (`GET /api/v3/workspaces/{workspace_id}/memories/facts`).
//!
//! The per-scope listings (`GET .../actors/{id}/facts`,
//! `.../projects/{id}/memories/facts`, `.../agents/{id}/facts`) are
//! deliberately not bound: this endpoint takes the same `fact_fuzzy` filter
//! and page-size range, covers any mix of the three scopes in one call, and
//! additionally tags every fact with its owner.

use serde::{Deserialize, Serialize};

use crate::client::Client;
use crate::error::Result;

use super::path::workspace_facts_path;
use super::types::Fact;

/// Largest page the API accepts; larger values answer `INVALID_ARGUMENT`
/// (measured 2026-10-09). The server default is 50.
pub const MAX_FACT_PAGE_SIZE: u32 = 200;

/// Most distinct owners (actors, projects, and agents together) one listing
/// may name, per the API documentation.
pub const MAX_FACT_LIST_OWNERS: usize = 50;

/// Paginated fact list payload.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct FactList {
    /// Facts on this page, newest first.
    #[serde(default)]
    pub items: Vec<Fact>,
    /// Exact cross-page count, when the server provides it.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub total: Option<u64>,
    /// Token for the next page, if any.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub continuation_token: Option<String>,
}

/// Query parameters for listing facts.
///
/// The endpoint requires **at least one** of `actor_ids` / `project_ids` /
/// `agent_ids`, naming at most [`MAX_FACT_LIST_OWNERS`] distinct owners in
/// total. With no owner it used to answer an empty page (measured
/// 2026-08-07) and now answers `INVALID_ARGUMENT` (measured 2026-10-09).
/// Callers enforce both limits before building a request.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct ListFactsParams {
    /// Limit to facts owned by these actors.
    pub actor_ids: Vec<String>,
    /// Limit to facts owned by these projects.
    pub project_ids: Vec<String>,
    /// Limit to facts owned by these agents.
    ///
    /// An agent's facts are also reachable through `actor_ids` with the
    /// agent's own actor id; they come back tagged with the agent as owner
    /// either way (measured 2026-10-09).
    pub agent_ids: Vec<String>,
    /// Keep only facts whose text contains this substring.
    pub fact_fuzzy: Option<String>,
    /// Page size, 1 to [`MAX_FACT_PAGE_SIZE`]. The server defaults to 50.
    pub page_size: Option<u32>,
    /// Continuation token from a previous page.
    pub continuation_token: Option<String>,
}

impl ListFactsParams {
    /// Render as client query pairs, omitting unset values.
    ///
    /// List filters repeat the key once per value (`actor_ids=a&actor_ids=b`),
    /// which is how the API reads array parameters.
    fn to_query(&self) -> Vec<(&'static str, String)> {
        let mut query = Vec::new();
        for actor_id in &self.actor_ids {
            query.push(("actor_ids", actor_id.clone()));
        }
        for project_id in &self.project_ids {
            query.push(("project_ids", project_id.clone()));
        }
        for agent_id in &self.agent_ids {
            query.push(("agent_ids", agent_id.clone()));
        }
        if let Some(fuzzy) = &self.fact_fuzzy {
            query.push(("fact_fuzzy", fuzzy.clone()));
        }
        if let Some(page_size) = self.page_size {
            query.push(("page_size", page_size.to_string()));
        }
        if let Some(token) = &self.continuation_token {
            query.push(("continuation_token", token.clone()));
        }
        query
    }
}

/// List facts across the workspace, filtered by owning scope.
///
/// Paging is not performed automatically: pass the returned
/// [`continuation_token`](FactList::continuation_token) back to fetch the next
/// page.
pub fn list_facts(
    client: &Client,
    workspace_id: &str,
    params: &ListFactsParams,
) -> Result<FactList> {
    client.get_data(&workspace_facts_path(workspace_id), &params.to_query())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn list_filters_repeat_the_key_per_value() {
        let params = ListFactsParams {
            actor_ids: vec!["actor-1".into(), "actor-2".into()],
            project_ids: vec!["proj-1".into()],
            agent_ids: vec!["agent-1".into()],
            fact_fuzzy: Some("editor".into()),
            page_size: Some(200),
            continuation_token: Some("tok".into()),
        };
        assert_eq!(
            params.to_query(),
            vec![
                ("actor_ids", "actor-1".to_string()),
                ("actor_ids", "actor-2".to_string()),
                ("project_ids", "proj-1".to_string()),
                ("agent_ids", "agent-1".to_string()),
                ("fact_fuzzy", "editor".to_string()),
                ("page_size", "200".to_string()),
                ("continuation_token", "tok".to_string()),
            ]
        );
    }

    #[test]
    fn default_params_send_nothing() {
        assert!(ListFactsParams::default().to_query().is_empty());
    }

    #[test]
    fn a_listing_page_decodes_items_total_and_token() {
        // Shape measured live 2026-08-07.
        let page: FactList = serde_json::from_str(
            r#"{
                "items": [{
                    "id": "fact-531bbd6f095d49a8a8f8d5809945a263",
                    "fact": "second",
                    "owner": {"type": "actor", "id": "actor-8c588aff93d346479b8ad4e56ec3f860"}
                }],
                "total": 2,
                "continuation_token": "tok-next"
            }"#,
        )
        .expect("decode");
        assert_eq!(page.items.len(), 1);
        assert_eq!(page.total, Some(2));
        assert_eq!(page.continuation_token.as_deref(), Some("tok-next"));
    }

    #[test]
    fn an_empty_page_decodes_with_every_field_absent() {
        let page: FactList = serde_json::from_str("{}").expect("decode");
        assert!(page.items.is_empty());
        assert_eq!(page.total, None);
        assert_eq!(page.continuation_token, None);
    }
}
