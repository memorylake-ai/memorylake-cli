//! Get one agent version (`GET /api/v3/agents/{id}/versions/{version}`).

use crate::client::Client;
use crate::error::Result;

use super::agent_version_path;
use super::types::AgentVersion;

/// Fetch a specific configuration version of an agent.
pub fn get_agent_version(client: &Client, id: &str, version: u64) -> Result<AgentVersion> {
    client.get_data(&agent_version_path(id, version), &[])
}
