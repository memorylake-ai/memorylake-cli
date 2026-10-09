//! Get a database memory
//! (`GET /api/v3/workspaces/{workspace_id}/projects/{project_id}/memories/databases/{database_id}`).

use crate::client::Client;
use crate::error::Result;

use super::path::database_path;
use super::types::DatabaseMemory;

/// Fetch a database memory, including its `instruction`.
pub fn get_database_memory(
    client: &Client,
    workspace_id: &str,
    project_id: &str,
    database_id: &str,
) -> Result<DatabaseMemory> {
    client.get_data(&database_path(workspace_id, project_id, database_id), &[])
}
