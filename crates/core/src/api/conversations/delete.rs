//! Delete a conversation
//! (`DELETE /api/v3/workspaces/{workspace_id}/memories/conversations/{id}`).

use serde_json::Value;

use crate::client::Client;
use crate::error::Result;

use super::path::conversation_path;

/// Permanently delete a conversation and every message in it.
///
/// This is irreversible. The endpoint takes a server-assigned id only — unlike
/// get and cook-status it has no `by_custom_id` lookup, so resolve a
/// `custom_id` through
/// [`get_conversation_by_custom_id`](super::get_conversation_by_custom_id)
/// first.
///
/// The endpoint answers with an empty `data` object. Decoding into [`Value`]
/// rather than a fixed shape keeps a future non-empty payload from turning a
/// successful delete into a decode error; the envelope itself is still fully
/// validated by the client.
pub fn delete_conversation(
    client: &Client,
    workspace_id: &str,
    conversation_id: &str,
) -> Result<()> {
    let _: Value = client.delete_data(&conversation_path(workspace_id, conversation_id))?;
    Ok(())
}
