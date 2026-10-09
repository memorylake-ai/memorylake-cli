//! Delete a knowledge entry
//! (`DELETE /api/v3/workspaces/{workspace_id}/analysis-models/{model_id}/entries/{entry_id}`).

use crate::client::Client;
use crate::error::Result;

use super::path::{entry_path, with_entity_type};

/// Delete one knowledge entry.
///
/// Whether a delete is allowed depends on the kind's `delete_validation` rule
/// (see the templates endpoint): some kinds may only be deleted when nothing
/// else hangs off them. `entity_type` is optional, but some kinds cannot be
/// located from the id alone. Success carries no payload.
pub fn delete_entry(
    client: &Client,
    workspace_id: &str,
    model_id: &str,
    entry_id: &str,
    entity_type: Option<&str>,
) -> Result<()> {
    client.delete_empty(&with_entity_type(
        entry_path(workspace_id, model_id, entry_id),
        entity_type,
    ))
}
