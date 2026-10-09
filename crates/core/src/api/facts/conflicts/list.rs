//! List one scope's memory conflicts (`GET .../memories/conflicts`).

use serde::{Deserialize, Serialize};

use crate::client::Client;
use crate::error::Result;

use super::super::path::conflicts_path;
use super::super::types::FactScope;
use super::types::{ConflictCategory, ConflictType, MemoryConflict};

/// Largest page the API accepts; larger values answer `INVALID_ARGUMENT`
/// (measured 2026-10-09). The server default is 20.
pub const MAX_CONFLICT_PAGE_SIZE: u32 = 100;

/// Paginated conflict list payload.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct ConflictList {
    /// Conflicts on this page, newest first.
    #[serde(default)]
    pub items: Vec<MemoryConflict>,
    /// Exact cross-page count, when the server provides it.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub total: Option<u64>,
    /// Token for the next page, if any.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub continuation_token: Option<String>,
}

/// Query parameters for listing conflicts. Every filter is optional and set
/// filters combine with AND; an unset boolean filter returns both states.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct ListConflictsParams {
    /// Keep only resolved (`true`) or unresolved (`false`) conflicts.
    pub resolved: Option<bool>,
    /// Keep only conflicts of this category.
    pub category: Option<ConflictCategory>,
    /// Keep only conflicts of this kind.
    pub conflict_type: Option<ConflictType>,
    /// Keep only conflicts whose facts did (`true`) or did not (`false`)
    /// change after detection.
    pub stale: Option<bool>,
    /// Page size, 1 to [`MAX_CONFLICT_PAGE_SIZE`]. The server defaults to 20.
    pub page_size: Option<u32>,
    /// Continuation token from a previous page.
    pub continuation_token: Option<String>,
}

impl ListConflictsParams {
    /// Render as client query pairs, omitting unset values.
    fn to_query(&self) -> Vec<(&'static str, String)> {
        let mut query = Vec::new();
        if let Some(resolved) = self.resolved {
            query.push(("resolved", resolved.to_string()));
        }
        if let Some(category) = self.category {
            query.push(("category", category.as_wire().to_string()));
        }
        if let Some(conflict_type) = self.conflict_type {
            query.push(("conflict_type", conflict_type.as_wire().to_string()));
        }
        if let Some(stale) = self.stale {
            query.push(("stale", stale.to_string()));
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

/// List the memory conflicts detected in one scope, newest first.
///
/// Detection runs server-side shortly after facts are stored — within about a
/// minute in every observed case (measured 2026-10-09).
pub fn list_conflicts(
    client: &Client,
    workspace_id: &str,
    scope: &FactScope,
    params: &ListConflictsParams,
) -> Result<ConflictList> {
    client.get_data(&conflicts_path(workspace_id, scope), &params.to_query())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_filter_reaches_the_query_under_its_wire_name() {
        let params = ListConflictsParams {
            resolved: Some(false),
            category: Some(ConflictCategory::SelfContradiction),
            conflict_type: Some(ConflictType::Logical),
            stale: Some(true),
            page_size: Some(100),
            continuation_token: Some("tok".into()),
        };
        assert_eq!(
            params.to_query(),
            vec![
                ("resolved", "false".to_string()),
                ("category", "self".to_string()),
                ("conflict_type", "logical".to_string()),
                ("stale", "true".to_string()),
                ("page_size", "100".to_string()),
                ("continuation_token", "tok".to_string()),
            ]
        );
    }

    #[test]
    fn default_params_send_nothing() {
        assert!(ListConflictsParams::default().to_query().is_empty());
    }

    #[test]
    fn an_empty_page_decodes() {
        // What an empty scope answers (measured 2026-10-09).
        let page: ConflictList =
            serde_json::from_str(r#"{"items": [], "total": 0}"#).expect("decode");
        assert!(page.items.is_empty());
        assert_eq!(page.total, Some(0));
        assert_eq!(page.continuation_token, None);
    }
}
