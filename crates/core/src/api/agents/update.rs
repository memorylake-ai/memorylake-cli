//! Update agent identity (`PATCH /api/v3/agents/{id}`).

use crate::client::Client;
use crate::error::Result;

use super::agent_path;
use super::types::{Agent, AgentRequestBody};

/// Update an agent's identity fields.
///
/// The endpoint accepts `name`, `description`, and `metadata` only; `metadata`
/// replaces the stored object outright rather than merging into it. Changing
/// model, policies, prompt, or any other configuration requires
/// [`super::create_agent_version`] instead — see [`super::CONFIG_FIELDS`].
pub fn update_agent(client: &Client, id: &str, body: &AgentRequestBody) -> Result<Agent> {
    client.patch_data(&agent_path(id), body)
}
