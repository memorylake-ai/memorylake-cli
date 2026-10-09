//! Delete a datasource
//! (`DELETE /api/v3/workspaces/{workspace_id}/db-datasources/{datasource_id}`).

use crate::client::Client;
use crate::error::Result;

use super::path::db_datasource_path;

/// Delete a datasource.
///
/// Refused while any analysis model or database memory still uses it. An
/// unknown id is `NOT_FOUND` (measured 2026-10-09).
pub fn delete_db_datasource(
    client: &Client,
    workspace_id: &str,
    datasource_id: &str,
) -> Result<()> {
    client.delete_empty(&db_datasource_path(workspace_id, datasource_id))
}
