//! List a project's database memories
//! (`GET /api/v3/workspaces/{workspace_id}/projects/{project_id}/memories/databases`).

use serde::{Deserialize, Serialize};

use crate::client::Client;
use crate::error::Result;

use super::path::databases_path;
use super::types::DatabaseMemory;

/// Paginated database-memory list payload.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct DatabaseMemoryList {
    /// Database memories on this page. `instruction` is not included here.
    #[serde(default)]
    pub items: Vec<DatabaseMemory>,
    /// Exact cross-page count, when the server provides it.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub total: Option<u64>,
    /// Token for the next page; absent on the last one.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub continuation_token: Option<String>,
}

/// Query parameters for listing database memories.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct ListDatabaseMemoriesParams {
    /// Page size, 1-100. The server defaults to 20.
    pub page_size: Option<u32>,
    /// Continuation token from a previous page.
    pub continuation_token: Option<String>,
}

impl ListDatabaseMemoriesParams {
    /// Render as client query pairs, omitting unset values.
    fn to_query(&self) -> Vec<(&'static str, String)> {
        let mut query = Vec::new();
        if let Some(page_size) = self.page_size {
            query.push(("page_size", page_size.to_string()));
        }
        if let Some(token) = &self.continuation_token {
            query.push(("continuation_token", token.clone()));
        }
        query
    }
}

/// List the database memories in a project.
pub fn list_database_memories(
    client: &Client,
    workspace_id: &str,
    project_id: &str,
    params: &ListDatabaseMemoriesParams,
) -> Result<DatabaseMemoryList> {
    client.get_data(
        &databases_path(workspace_id, project_id),
        &params.to_query(),
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn to_query_omits_unset_params() {
        assert!(ListDatabaseMemoriesParams::default().to_query().is_empty());
    }

    #[test]
    fn to_query_renders_paging_params() {
        let params = ListDatabaseMemoriesParams {
            page_size: Some(5),
            continuation_token: Some("tok".into()),
        };
        assert_eq!(
            params.to_query(),
            vec![
                ("page_size", "5".to_string()),
                ("continuation_token", "tok".to_string()),
            ]
        );
    }

    #[test]
    fn the_measured_empty_list_decodes() {
        // Production on 2026-10-09: unlike the connection and datasource
        // lists, this one does report `total`.
        let list: DatabaseMemoryList = serde_json::from_str(r#"{"items":[],"total":0}"#).unwrap();
        assert!(list.items.is_empty());
        assert_eq!(list.total, Some(0));
    }
}
