//! Delete a project
//! (`DELETE /api/v3/workspaces/{workspace_id}/projects/{project_id}`).

use serde_json::Value;

use crate::client::Client;
use crate::error::Result;

use super::path::project_path;

/// Permanently delete a project.
///
/// This is irreversible: the API removes the project's documents and
/// conversations along with it.
///
/// The endpoint answers with an empty `data` object. Decoding into [`Value`]
/// rather than a fixed shape keeps a future non-empty payload from turning a
/// successful delete into a decode error; the envelope itself is still fully
/// validated by the client.
pub fn delete_project(client: &Client, workspace_id: &str, project_id: &str) -> Result<()> {
    let _: Value = client.delete_data(&project_path(workspace_id, project_id))?;
    Ok(())
}
