//! Delete a database memory
//! (`DELETE /api/v3/workspaces/{workspace_id}/projects/{project_id}/memories/databases/{database_id}`).

use crate::client::Client;
use crate::error::Result;

use super::path::database_path;

/// Remove a database memory from a project. The datasource it read is kept.
///
/// Idempotent on production: an id that does not exist still answers
/// `{"success":true}` (measured 2026-10-09), unlike the connection and
/// datasource deletes, which answer `NOT_FOUND`. Success therefore does not
/// prove the id was ever there.
pub fn delete_database_memory(
    client: &Client,
    workspace_id: &str,
    project_id: &str,
    database_id: &str,
) -> Result<()> {
    client.delete_empty(&database_path(workspace_id, project_id, database_id))
}
