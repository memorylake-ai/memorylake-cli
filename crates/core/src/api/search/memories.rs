//! Search memories
//! (`POST /api/v3/workspaces/{workspace_id}/memories/search`).

use serde::Serialize;

use crate::api::path::encode_segment;
use crate::client::Client;
use crate::error::Result;

use super::types::{MemoryType, SearchResults};

fn search_path(workspace_id: &str) -> String {
    format!(
        "/api/v3/workspaces/{}/memories/search",
        encode_segment(workspace_id)
    )
}

/// Request body for a memory search.
///
/// Every filter is skipped when `None`: the API treats an omitted filter as
/// "no restriction", so sending `null` or `[]` would say something different.
/// `top_k` is likewise omitted rather than defaulted here — the API documents
/// no default, and guessing one client-side would silently override it.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct SearchMemoriesRequest {
    /// Natural language query.
    pub query: String,
    /// Limit to these projects. Omitted means every project in the workspace.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub project_ids: Option<Vec<String>>,
    /// Limit to memories associated with these actors.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub actor_ids: Option<Vec<String>>,
    /// Limit to these source types. Omitted means every type.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub memory_types: Option<Vec<MemoryType>>,
    /// Maximum results per source type.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub top_k: Option<u32>,
}

impl SearchMemoriesRequest {
    /// A request carrying only the query, with every filter left unset.
    pub fn new(query: impl Into<String>) -> Self {
        Self {
            query: query.into(),
            project_ids: None,
            actor_ids: None,
            memory_types: None,
            top_k: None,
        }
    }
}

/// Search documents and facts across one workspace.
///
/// The endpoint has no pagination; `top_k` caps results per source type.
pub fn search_memories(
    client: &Client,
    workspace_id: &str,
    request: &SearchMemoriesRequest,
) -> Result<SearchResults> {
    client.post_data(&search_path(workspace_id), request)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn search_path_leaves_typical_ids_untouched() {
        assert_eq!(
            search_path("ws-b83fa7f09f19487f9905888f35542849"),
            "/api/v3/workspaces/ws-b83fa7f09f19487f9905888f35542849/memories/search"
        );
    }

    #[test]
    fn search_path_escapes_url_structural_chars() {
        assert_eq!(
            search_path("weird id/here?foo#bar"),
            "/api/v3/workspaces/weird%20id%2Fhere%3Ffoo%23bar/memories/search"
        );
    }

    #[test]
    fn search_path_escapes_percent_itself() {
        assert_eq!(
            search_path("100%off"),
            "/api/v3/workspaces/100%25off/memories/search"
        );
    }

    #[test]
    fn query_only_request_sends_just_the_query() {
        let body = serde_json::to_string(&SearchMemoriesRequest::new("quarterly revenue"))
            .expect("serialize");
        assert_eq!(body, r#"{"query":"quarterly revenue"}"#);
    }

    #[test]
    fn unset_filters_are_absent_rather_than_null_or_empty() {
        let body = serde_json::to_value(&SearchMemoriesRequest {
            query: "q".to_string(),
            project_ids: None,
            actor_ids: None,
            memory_types: None,
            top_k: None,
        })
        .expect("serialize");
        let object = body.as_object().expect("object");
        assert_eq!(object.len(), 1, "only `query` should be sent: {body}");
        for key in ["project_ids", "actor_ids", "memory_types", "top_k"] {
            assert!(!object.contains_key(key), "{key} should be absent: {body}");
        }
    }

    #[test]
    fn every_filter_serializes_with_its_documented_name() {
        let body = serde_json::to_value(&SearchMemoriesRequest {
            query: "q".to_string(),
            project_ids: Some(vec!["proj-1".into(), "proj-2".into()]),
            actor_ids: Some(vec!["act-1".into()]),
            memory_types: Some(vec![MemoryType::Document, MemoryType::Fact]),
            top_k: Some(10),
        })
        .expect("serialize");

        assert_eq!(
            body,
            serde_json::json!({
                "query": "q",
                "project_ids": ["proj-1", "proj-2"],
                "actor_ids": ["act-1"],
                "memory_types": ["document", "fact"],
                "top_k": 10
            })
        );
    }

    #[test]
    fn an_explicitly_empty_filter_list_is_still_sent() {
        // The CLI rejects empty lists before this point; if some other caller
        // sets one deliberately, it must not be silently dropped.
        let body = serde_json::to_value(&SearchMemoriesRequest {
            project_ids: Some(Vec::new()),
            ..SearchMemoriesRequest::new("q")
        })
        .expect("serialize");
        assert_eq!(body["project_ids"], serde_json::json!([]));
    }
}
