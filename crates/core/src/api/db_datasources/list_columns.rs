//! List one table's columns
//! (`GET /api/v3/workspaces/{workspace_id}/db-datasources/{datasource_id}/columns`).

use serde::{Deserialize, Serialize};

use crate::client::Client;
use crate::error::Result;

use super::path::db_datasource_child_path;
use super::types::DbColumn;

/// Column list payload.
///
/// The endpoint returns every column in one response; the paging fields of
/// the shared list shape are modeled only so nothing the server sends is
/// dropped.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct DbColumnList {
    /// Columns of the requested table.
    #[serde(default)]
    pub items: Vec<DbColumn>,
    /// Count, when the server provides it.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub total: Option<u64>,
    /// Never set in practice: there is no next page.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub continuation_token: Option<String>,
}

/// List the columns of `table_name`, which the API requires
/// (`INVALID_ARGUMENT` without it, measured 2026-10-09).
pub fn list_db_datasource_columns(
    client: &Client,
    workspace_id: &str,
    datasource_id: &str,
    table_name: &str,
) -> Result<DbColumnList> {
    client.get_data(
        &db_datasource_child_path(workspace_id, datasource_id, "columns"),
        &[("table_name", table_name.to_string())],
    )
}
