//! List database connections (`GET /api/v3/db-connections`).

use serde::{Deserialize, Serialize};

use crate::client::Client;
use crate::error::Result;

use super::DB_CONNECTIONS_PATH;
use super::types::DbConnection;

/// Paginated connection list payload.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct DbConnectionList {
    /// Connections on this page.
    #[serde(default)]
    pub items: Vec<DbConnection>,
    /// Exact cross-page count, when the server provides it. Absent when
    /// visibility is decided per row; page with the token instead.
    #[serde(
        default,
        deserialize_with = "crate::api::lenient::int",
        skip_serializing_if = "Option::is_none"
    )]
    pub total: Option<i64>,
    /// Token for the next page; absent on the last one.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub continuation_token: Option<String>,
}

/// Query parameters for listing connections.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct ListDbConnectionsParams {
    /// Page size, 1-100. The server defaults to 20.
    pub page_size: Option<u32>,
    /// Continuation token from a previous page.
    pub continuation_token: Option<String>,
    /// Case-insensitive substring filter on the name. Sent as `name_fuzzy`.
    pub name_fuzzy: Option<String>,
}

impl ListDbConnectionsParams {
    /// Render as client query pairs, omitting unset values.
    fn to_query(&self) -> Vec<(&'static str, String)> {
        let mut query = Vec::new();
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

/// List the connections the caller may open.
///
/// Paging is not performed automatically: pass the returned
/// [`continuation_token`](DbConnectionList::continuation_token) back for the
/// next page.
pub fn list_db_connections(
    client: &Client,
    params: &ListDbConnectionsParams,
) -> Result<DbConnectionList> {
    client.get_data(DB_CONNECTIONS_PATH, &params.to_query())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn to_query_omits_unset_params() {
        assert!(ListDbConnectionsParams::default().to_query().is_empty());
    }

    #[test]
    fn to_query_renders_every_param_under_its_documented_name() {
        let params = ListDbConnectionsParams {
            page_size: Some(50),
            continuation_token: Some("tok".into()),
            name_fuzzy: Some("analytics".into()),
        };
        assert_eq!(
            params.to_query(),
            vec![
                ("page_size", "50".to_string()),
                ("continuation_token", "tok".to_string()),
                ("name_fuzzy", "analytics".to_string()),
            ]
        );
    }

    #[test]
    fn the_measured_empty_list_decodes() {
        // Production on 2026-10-09: `{"items":[]}`, with neither `total` nor
        // `continuation_token`.
        let list: DbConnectionList = serde_json::from_str(r#"{"items":[]}"#).expect("decode");
        assert!(list.items.is_empty());
        assert!(list.total.is_none());
        assert!(list.continuation_token.is_none());
    }

    #[test]
    fn a_page_with_total_and_token_decodes() {
        let list: DbConnectionList = serde_json::from_str(
            r#"{"items":[{"id":"c1","name":"a"}],"total":7,"continuation_token":"next"}"#,
        )
        .expect("decode");
        assert_eq!(list.items.len(), 1);
        assert_eq!(list.total, Some(7));
        assert_eq!(list.continuation_token.as_deref(), Some("next"));
    }
}
