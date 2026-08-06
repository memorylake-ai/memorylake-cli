//! Bind an agent to a workspace (`POST /api/v3/workspaces/{id}/agents`).

use serde::Serialize;

use crate::client::Client;
use crate::error::Result;

use super::types::WorkspaceAgentBinding;
use super::workspace_agents_path;

/// Request body for binding an agent to a workspace.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct BindAgentRequest {
    /// Identifier of the agent to bind.
    pub agent_id: String,
}

/// Bind an agent to a workspace so it can operate within it.
pub fn bind_agent(
    client: &Client,
    workspace_id: &str,
    request: &BindAgentRequest,
) -> Result<WorkspaceAgentBinding> {
    client.post_data(&workspace_agents_path(workspace_id), request)
}
