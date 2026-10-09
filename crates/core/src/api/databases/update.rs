//! Update a database memory
//! (`PATCH /api/v3/workspaces/{workspace_id}/projects/{project_id}/memories/databases/{database_id}`).

use serde::Serialize;

use crate::client::Client;
use crate::error::Result;

use super::path::database_path;
use super::types::DatabaseMemory;

/// Request body for updating a database memory.
///
/// Partial-update semantics: unset fields are left out of the body and stay as
/// they are. The datasource cannot be changed.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize)]
pub struct UpdateDatabaseMemoryRequest {
    /// New display name.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,
    /// New instruction, replacing the stored one.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub instruction: Option<String>,
    /// Analysis model binding: `Some(id)` binds one, `Some("")` removes the
    /// binding, `None` leaves it as it is.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub analysis_model_id: Option<String>,
}

impl UpdateDatabaseMemoryRequest {
    /// Whether the request would change nothing at all.
    pub fn is_empty(&self) -> bool {
        self.name.is_none() && self.instruction.is_none() && self.analysis_model_id.is_none()
    }
}

/// Update a database memory's name, instruction, or analysis model binding.
pub fn update_database_memory(
    client: &Client,
    workspace_id: &str,
    project_id: &str,
    database_id: &str,
    request: &UpdateDatabaseMemoryRequest,
) -> Result<DatabaseMemory> {
    client.patch_data(
        &database_path(workspace_id, project_id, database_id),
        request,
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn omitted_fields_are_absent_from_the_body() {
        let body = serde_json::to_string(&UpdateDatabaseMemoryRequest {
            instruction: Some("Use UTC.".into()),
            ..Default::default()
        })
        .unwrap();
        assert_eq!(body, r#"{"instruction":"Use UTC."}"#);
    }

    #[test]
    fn an_empty_analysis_model_id_removes_the_binding() {
        // The API's documented way to unbind; it must reach the wire as "",
        // not be dropped as though it were unset.
        let body = serde_json::to_string(&UpdateDatabaseMemoryRequest {
            analysis_model_id: Some(String::new()),
            ..Default::default()
        })
        .unwrap();
        assert_eq!(body, r#"{"analysis_model_id":""}"#);
    }

    #[test]
    fn only_the_default_request_is_empty() {
        assert!(UpdateDatabaseMemoryRequest::default().is_empty());
        assert!(
            !UpdateDatabaseMemoryRequest {
                analysis_model_id: Some(String::new()),
                ..Default::default()
            }
            .is_empty()
        );
    }
}
