//! Agents v3 API (`/api/v3/agents` and `/api/v3/workspaces/{id}/agents`).
//!
//! Agent *identity* (name, description, metadata) is mutable in place via
//! [`update_agent`]. Agent *configuration* (model, policies, prompt, …) is
//! immutable: changing it means creating a new version with
//! [`create_agent_version`].

mod bind;
mod create;
mod create_version;
mod delete;
mod get;
mod get_version;
mod list;
mod list_versions;
mod list_workspace_agents;
mod types;
mod unbind;
mod update;

pub use bind::{BindAgentRequest, bind_agent};
pub use create::create_agent;
pub use create_version::create_agent_version;
pub use delete::delete_agent;
pub use get::{get_agent, get_agent_by_custom_id};
pub use get_version::get_agent_version;
pub use list::{AgentList, ListAgentsParams, list_agents};
pub use list_versions::{AgentVersionList, ListAgentVersionsParams, list_agent_versions};
pub use list_workspace_agents::{WorkspaceAgentList, list_workspace_agents};
pub use types::{
    Agent, AgentRequestBody, AgentVersion, CONFIG_FIELDS, IDENTITY_FIELDS, WorkspaceAgentBinding,
};
pub use unbind::unbind_agent;
pub use update::update_agent;

use crate::api::path::encode_segment;

/// Agent collection endpoint.
///
/// Paths are relative to the configured base URL, which already carries the
/// `/openapi/memorylake` prefix — it must not be repeated here.
const AGENTS_PATH: &str = "/api/v3/agents";

/// `/api/v3/agents/{id}`
fn agent_path(id: &str) -> String {
    format!("{AGENTS_PATH}/{}", encode_segment(id))
}

/// `/api/v3/agents/{id}/versions`
fn agent_versions_path(id: &str) -> String {
    format!("{}/versions", agent_path(id))
}

/// `/api/v3/agents/{id}/versions/{version}`
fn agent_version_path(id: &str, version: u64) -> String {
    format!("{}/{version}", agent_versions_path(id))
}

/// `/api/v3/workspaces/{workspace_id}/agents`
fn workspace_agents_path(workspace_id: &str) -> String {
    format!("/api/v3/workspaces/{}/agents", encode_segment(workspace_id))
}

/// `/api/v3/workspaces/{workspace_id}/agents/{agent_id}`
fn workspace_agent_path(workspace_id: &str, agent_id: &str) -> String {
    format!(
        "{}/{}",
        workspace_agents_path(workspace_id),
        encode_segment(agent_id)
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn agent_paths_are_relative_to_the_base_url() {
        assert_eq!(agent_path("agt-1"), "/api/v3/agents/agt-1");
        assert_eq!(
            agent_versions_path("agt-1"),
            "/api/v3/agents/agt-1/versions"
        );
        assert_eq!(
            agent_version_path("agt-1", 7),
            "/api/v3/agents/agt-1/versions/7"
        );
    }

    #[test]
    fn workspace_agent_paths_are_relative_to_the_base_url() {
        assert_eq!(
            workspace_agents_path("ws-1"),
            "/api/v3/workspaces/ws-1/agents"
        );
        assert_eq!(
            workspace_agent_path("ws-1", "agt-1"),
            "/api/v3/workspaces/ws-1/agents/agt-1"
        );
    }

    #[test]
    fn ids_cannot_escape_their_path_segment() {
        assert_eq!(
            agent_path("../workspaces?x=1"),
            "/api/v3/agents/..%2Fworkspaces%3Fx=1"
        );
        assert_eq!(
            workspace_agent_path("ws/1", "agt#2"),
            "/api/v3/workspaces/ws%2F1/agents/agt%232"
        );
    }
}
