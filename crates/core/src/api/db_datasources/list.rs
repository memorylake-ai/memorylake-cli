//! List a workspace's datasources
//! (`GET /api/v3/workspaces/{workspace_id}/db-datasources`).

use serde::{Deserialize, Serialize};

use crate::client::Client;
use crate::error::Result;

use super::path::db_datasources_path;
use super::types::DbDatasource;

/// Paginated datasource list payload.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct DbDatasourceList {
    /// Datasources on this page.
    #[serde(default)]
    pub items: Vec<DbDatasource>,
    /// Exact cross-page count, when the server provides it.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub total: Option<u64>,
    /// Token for the next page; absent on the last one.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub continuation_token: Option<String>,
}

/// Query parameters for listing datasources.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct ListDbDatasourcesParams {
    /// Only datasources reading through this connection. Sent as
    /// `db_connection_id`.
    pub db_connection_id: Option<String>,
    /// Case-insensitive substring filter on the name. Sent as `name_fuzzy`.
    pub name_fuzzy: Option<String>,
    /// Page size, 1-100. The server defaults to 20.
    pub page_size: Option<u32>,
    /// Continuation token from a previous page.
    pub continuation_token: Option<String>,
}

impl ListDbDatasourcesParams {
    /// Render as client query pairs, omitting unset values.
    fn to_query(&self) -> Vec<(&'static str, String)> {
        let mut query = Vec::new();
        if let Some(connection) = &self.db_connection_id {
            query.push(("db_connection_id", connection.clone()));
        }
        if let Some(name_fuzzy) = &self.name_fuzzy {
            query.push(("name_fuzzy", name_fuzzy.clone()));
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

/// List the datasources in `workspace_id` that the caller may open.
pub fn list_db_datasources(
    client: &Client,
    workspace_id: &str,
    params: &ListDbDatasourcesParams,
) -> Result<DbDatasourceList> {
    client.get_data(&db_datasources_path(workspace_id), &params.to_query())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn to_query_omits_unset_params() {
        assert!(ListDbDatasourcesParams::default().to_query().is_empty());
    }

    #[test]
    fn to_query_renders_every_param_under_its_documented_name() {
        let params = ListDbDatasourcesParams {
            db_connection_id: Some("conn-1".into()),
            name_fuzzy: Some("sales".into()),
            page_size: Some(10),
            continuation_token: Some("tok".into()),
        };
        assert_eq!(
            params.to_query(),
            vec![
                ("db_connection_id", "conn-1".to_string()),
                ("name_fuzzy", "sales".to_string()),
                ("page_size", "10".to_string()),
                ("continuation_token", "tok".to_string()),
            ]
        );
    }

    #[test]
    fn the_measured_empty_list_decodes() {
        // Production on 2026-10-09: `{"items":[]}` and nothing else.
        let list: DbDatasourceList = serde_json::from_str(r#"{"items":[]}"#).unwrap();
        assert!(list.items.is_empty());
        assert!(list.total.is_none());
        assert!(list.continuation_token.is_none());
    }
}
