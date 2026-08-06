//! Unbind an agent from a workspace
//! (`DELETE /api/v3/workspaces/{id}/agents/{agentId}`).

use crate::client::Client;
use crate::error::Result;

use super::workspace_agent_path;

/// Remove the binding between an agent and a workspace.
///
/// The agent definition itself is left intact; only its availability inside
/// that workspace is removed. Success carries no payload, but the response
/// envelope is still checked.
pub fn unbind_agent(client: &Client, workspace_id: &str, agent_id: &str) -> Result<()> {
    client.delete_empty(&workspace_agent_path(workspace_id, agent_id))
}
