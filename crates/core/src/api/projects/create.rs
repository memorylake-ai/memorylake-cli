//! Create a project (`POST /api/v3/workspaces/{workspace_id}/projects`).

use serde::Serialize;

use crate::client::Client;
use crate::error::Result;

use super::path::projects_path;
use super::types::Project;

/// Request body for creating a project.
///
/// The endpoint also accepts `metadata`; it is not exposed yet, so it is never
/// sent.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct CreateProjectRequest {
    /// Display name.
    pub name: String,
    /// Caller-defined external id. Must be unique within the workspace.
    pub custom_id: String,
    /// Optional description.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    /// Opendata ids to attach (see [`crate::api::opendata`]). An unknown id
    /// is rejected with `INVALID_ARGUMENT`. Sent as `industry_ids`.
    #[serde(rename = "industry_ids", skip_serializing_if = "Option::is_none")]
    pub opendata_ids: Option<Vec<String>>,
}

/// Create a project inside `workspace_id`.
pub fn create_project(
    client: &Client,
    workspace_id: &str,
    request: &CreateProjectRequest,
) -> Result<Project> {
    client.post_data(&projects_path(workspace_id), request)
}
