//! Get a single project
//! (`GET /api/v3/workspaces/{workspace_id}/projects/{project_id}`).

use crate::client::Client;
use crate::error::Result;

use super::path::project_path;
use super::types::Project;

/// Fetch a project by its server-assigned id.
pub fn get_project(client: &Client, workspace_id: &str, project_id: &str) -> Result<Project> {
    client.get_data(&project_path(workspace_id, project_id), &[])
}

/// Fetch a project by the caller-defined `custom_id`.
///
/// `by_custom_id` is documented only for this endpoint; update and delete
/// address projects by their server-assigned id.
pub fn get_project_by_custom_id(
    client: &Client,
    workspace_id: &str,
    custom_id: &str,
) -> Result<Project> {
    client.get_data(
        &project_path(workspace_id, custom_id),
        &[("by_custom_id", "true".to_string())],
    )
}
