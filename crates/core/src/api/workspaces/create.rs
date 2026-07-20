//! Create a workspace (`POST /api/v3/workspaces`).

use serde::Serialize;

use crate::client::Client;
use crate::error::Result;

use super::types::Workspace;

/// Request body for creating a workspace.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct CreateWorkspaceRequest {
    /// Display name.
    pub name: String,
    /// Caller-defined unique external id.
    pub custom_id: String,
    /// Optional description.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
}

/// Create a workspace.
pub fn create_workspace(client: &Client, request: &CreateWorkspaceRequest) -> Result<Workspace> {
    client.post_data("/api/v3/workspaces", request)
}
