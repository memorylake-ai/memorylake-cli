//! Register a database memory
//! (`POST /api/v3/workspaces/{workspace_id}/projects/{project_id}/memories/databases`).

use serde::Serialize;

use crate::client::Client;
use crate::error::Result;

use super::path::databases_path;
use super::types::DatabaseMemory;

/// Request body for creating a database memory.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct CreateDatabaseMemoryRequest {
    /// Display name.
    pub name: String,
    /// Datasource to read. Must belong to the project's workspace, and the
    /// caller needs `db_datasource:read` on it.
    pub db_datasource_id: String,
    /// Guidance for answering questions against this database. Optional; can
    /// be added later or drafted with [`super::generate_database_instruction`].
    #[serde(skip_serializing_if = "Option::is_none")]
    pub instruction: Option<String>,
}

/// Register a datasource as a memory source in a project.
pub fn create_database_memory(
    client: &Client,
    workspace_id: &str,
    project_id: &str,
    request: &CreateDatabaseMemoryRequest,
) -> Result<DatabaseMemory> {
    client.post_data(&databases_path(workspace_id, project_id), request)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn required_fields_only_serialize_without_optional_keys() {
        let body = serde_json::to_value(CreateDatabaseMemoryRequest {
            name: "analytics_db".into(),
            db_datasource_id: "ds-1".into(),
            instruction: None,
        })
        .unwrap();
        assert_eq!(
            body,
            serde_json::json!({"name": "analytics_db", "db_datasource_id": "ds-1"})
        );
    }

    #[test]
    fn the_instruction_serializes_when_set() {
        let body = serde_json::to_value(CreateDatabaseMemoryRequest {
            name: "analytics_db".into(),
            db_datasource_id: "ds-1".into(),
            instruction: Some("Revenue is in cents.".into()),
        })
        .unwrap();
        assert_eq!(body["instruction"], "Revenue is in cents.");
    }
}
