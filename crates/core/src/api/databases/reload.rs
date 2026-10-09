//! Refresh a database memory's schema
//! (`POST .../memories/databases/{database_id}/reload`).

use serde_json::Value;

use crate::client::Client;
use crate::error::Result;

use super::path::reload_path;

/// Trigger a refresh of the database's schema and table metadata.
///
/// The endpoint takes no body and answers with an empty envelope; an empty
/// JSON object is sent and whatever `data` holds is discarded, while a failed
/// envelope is still an error.
pub fn reload_database_memory(
    client: &Client,
    workspace_id: &str,
    project_id: &str,
    database_id: &str,
) -> Result<()> {
    client.post_data::<Value, _>(
        &reload_path(workspace_id, project_id, database_id),
        &serde_json::json!({}),
    )?;
    Ok(())
}
