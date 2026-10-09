//! List boundaries (`GET /api/v3/boundaries?workspace_id=...`).

use serde::{Deserialize, Serialize};

use crate::client::Client;
use crate::error::Result;

use super::BOUNDARIES_PATH;
use super::types::Boundary;

/// Paginated boundary list payload.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct BoundaryList {
    /// Boundaries on this page.
    #[serde(default)]
    pub items: Vec<Boundary>,
    /// Total matching boundaries across all pages, when the server reports it.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub total: Option<u64>,
    /// Token for the next page, if any.
    #[serde(default)]
    pub continuation_token: Option<String>,
}

/// Query parameters for listing boundaries, besides the required workspace.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct ListBoundariesParams {
    /// Page size (1–100; the server defaults to 20).
    pub page_size: Option<u32>,
    /// Continuation token from a previous page.
    pub continuation_token: Option<String>,
    /// Fuzzy filter by boundary name. Sent as `name_fuzzy`.
    pub name_fuzzy: Option<String>,
}

impl ListBoundariesParams {
    /// Render the workspace and the non-empty parameters as query pairs.
    fn to_query(&self, workspace_id: &str) -> Vec<(&'static str, String)> {
        let mut query = vec![("workspace_id", workspace_id.to_string())];
        if let Some(page_size) = self.page_size {
            query.push(("page_size", page_size.to_string()));
        }
        if let Some(token) = &self.continuation_token {
            query.push(("continuation_token", token.clone()));
        }
        if let Some(name_fuzzy) = &self.name_fuzzy {
            query.push(("name_fuzzy", name_fuzzy.clone()));
        }
        query
    }
}

/// List the boundaries in `workspace_id` that the caller may open.
pub fn list_boundaries(
    client: &Client,
    workspace_id: &str,
    params: &ListBoundariesParams,
) -> Result<BoundaryList> {
    client.get_data(BOUNDARIES_PATH, &params.to_query(workspace_id))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn to_query_always_carries_the_workspace() {
        assert_eq!(
            ListBoundariesParams::default().to_query("ws-1"),
            vec![("workspace_id", "ws-1".to_string())]
        );
    }

    #[test]
    fn to_query_renders_all_parameters() {
        let params = ListBoundariesParams {
            page_size: Some(5),
            continuation_token: Some("tok".into()),
            name_fuzzy: Some("scope".into()),
        };
        assert_eq!(
            params.to_query("ws-1"),
            vec![
                ("workspace_id", "ws-1".to_string()),
                ("page_size", "5".to_string()),
                ("continuation_token", "tok".to_string()),
                ("name_fuzzy", "scope".to_string()),
            ]
        );
    }
}
