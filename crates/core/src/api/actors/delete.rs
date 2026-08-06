//! Delete an actor (`DELETE /api/v3/actors/{id}`).

use crate::client::Client;
use crate::error::Result;

use super::get::actor_path;

/// Delete an actor.
///
/// Irreversible. Existing memories and conversation history survive but can no
/// longer be referenced, and every workspace binding for this actor is removed.
/// The API answers with `{"success": true, "message": ...}` and no `data`.
pub fn delete_actor(client: &Client, id: &str) -> Result<()> {
    client.delete_data(&actor_path(id))
}
