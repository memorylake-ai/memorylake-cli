//! Delete an agent (`DELETE /api/v3/agents/{id}`).

use crate::client::Client;
use crate::error::Result;

use super::agent_path;

/// Delete an agent.
///
/// Irreversible: the agent and all of its versions are removed. Workspace
/// bindings are kept rather than removed: they stay listed with
/// `status: INACTIVE` (read-only) until `unbind_agent` is called. Success carries no payload, but the response envelope
/// is still checked, so a server-side failure is reported as an error.
pub fn delete_agent(client: &Client, id: &str) -> Result<()> {
    client.delete_empty(&agent_path(id))
}
