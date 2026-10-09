//! List a datasource's tables
//! (`GET /api/v3/workspaces/{workspace_id}/db-datasources/{datasource_id}/tables`).

use serde::{Deserialize, Serialize};

use crate::client::Client;
use crate::error::Result;

use super::path::db_datasource_child_path;
use super::types::DbTable;

/// Table list payload.
///
/// The endpoint returns every table in one response; the paging fields of the
/// shared list shape are modeled only so nothing the server sends is dropped.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct DbTableList {
    /// Tables the datasource covers.
    #[serde(default)]
    pub items: Vec<DbTable>,
    /// Count, when the server provides it.
    #[serde(
        default,
        deserialize_with = "crate::api::lenient::int",
        skip_serializing_if = "Option::is_none"
    )]
    pub total: Option<i64>,
    /// Never set in practice: there is no next page.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub continuation_token: Option<String>,
}

/// List the tables a datasource covers, optionally filtered by a
/// case-insensitive substring of the table name.
pub fn list_db_datasource_tables(
    client: &Client,
    workspace_id: &str,
    datasource_id: &str,
    name_fuzzy: Option<&str>,
) -> Result<DbTableList> {
    let query: Vec<(&str, String)> = name_fuzzy
        .map(|name| vec![("name_fuzzy", name.to_string())])
        .unwrap_or_default();
    client.get_data(
        &db_datasource_child_path(workspace_id, datasource_id, "tables"),
        &query,
    )
}
