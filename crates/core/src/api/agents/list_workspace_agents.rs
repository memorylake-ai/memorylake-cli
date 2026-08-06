//! List the agents bound to a workspace
//! (`GET /api/v3/workspaces/{id}/agents`).

use serde::{Deserialize, Serialize};

use crate::client::Client;
use crate::error::Result;

use super::list::ListAgentsParams;
use super::types::WorkspaceAgentBinding;
use super::workspace_agents_path;

/// Paginated list of agents bound to a workspace.
///
/// Items are binding summaries, not full [`super::Agent`] objects.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct WorkspaceAgentList {
    /// Bindings on this page.
    #[serde(default)]
    pub items: Vec<WorkspaceAgentBinding>,
    /// Token for the next page, if any.
    #[serde(default)]
    pub continuation_token: Option<String>,
}

/// List every agent bound to `workspace_id`.
pub fn list_workspace_agents(
    client: &Client,
    workspace_id: &str,
    params: &ListAgentsParams,
) -> Result<WorkspaceAgentList> {
    client.get_data(&workspace_agents_path(workspace_id), &params.to_query())
}
