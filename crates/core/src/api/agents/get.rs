//! Get a single agent (`GET /api/v3/agents/{id}`).

use crate::client::Client;
use crate::error::Result;

use super::agent_path;
use super::types::Agent;

/// Fetch an agent by its server-assigned id.
pub fn get_agent(client: &Client, id: &str) -> Result<Agent> {
    client.get_data(&agent_path(id), &[])
}

/// Fetch an agent by the caller-defined `custom_id`.
pub fn get_agent_by_custom_id(client: &Client, custom_id: &str) -> Result<Agent> {
    client.get_data(
        &agent_path(custom_id),
        &[("by_custom_id", "true".to_string())],
    )
}
