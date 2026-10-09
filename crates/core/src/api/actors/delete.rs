//! Delete an actor (`DELETE /api/v3/actors/{id}`).

use crate::client::Client;
use crate::error::Result;

use super::get::actor_path;

/// Delete an actor.
///
/// Irreversible. Existing memories and conversation history survive but can no
/// longer be referenced. Workspace bindings are kept rather than removed: they
/// stay listed with `status: INACTIVE`.
///
/// Documented to answer with an empty `data` object, which [`Client::delete_empty`]
/// accepts whether or not the field is present.
pub fn delete_actor(client: &Client, id: &str) -> Result<()> {
    client.delete_empty(&actor_path(id))
}
