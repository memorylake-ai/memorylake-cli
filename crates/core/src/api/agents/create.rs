//! Create an agent (`POST /api/v3/agents`).

use crate::client::Client;
use crate::error::Result;

use super::AGENTS_PATH;
use super::types::{Agent, AgentRequestBody};

/// Create an agent.
///
/// `body` requires at least `name` and `custom_id`; every other documented
/// field is optional. Unrecognized keys are sent to the server unchanged — the
/// CLI does not gate on a closed field list. Creating an agent also generates
/// an Actor identity for it, returned as [`Agent::actor_id`].
pub fn create_agent(client: &Client, body: &AgentRequestBody) -> Result<Agent> {
    client.post_data(AGENTS_PATH, body)
}
