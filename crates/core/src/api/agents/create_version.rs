//! Create an agent version (`POST /api/v3/agents/{id}/versions`).

use crate::client::Client;
use crate::error::Result;

use super::agent_versions_path;
use super::types::{AgentRequestBody, AgentVersion};

/// Create a new configuration version for an agent.
///
/// `body` carries configuration fields (`model`, `capabilities`, `policies`,
/// `output`, `subagents`, `skills`, `system_prompt`, `model_settings`,
/// `runtime_bindings`). Identity fields are not versioned — use
/// [`super::update_agent`] for those.
pub fn create_agent_version(
    client: &Client,
    id: &str,
    body: &AgentRequestBody,
) -> Result<AgentVersion> {
    client.post_data(&agent_versions_path(id), body)
}
