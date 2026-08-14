//! Get a single conversation
//! (`GET /api/v3/workspaces/{workspace_id}/memories/conversations/{id}`).

use crate::client::Client;
use crate::error::Result;

use super::path::conversation_path;
use super::types::Conversation;

/// Query pair that switches the last path segment to a `custom_id` lookup.
pub(super) fn by_custom_id_query() -> [(&'static str, String); 1] {
    [("by_custom_id", "true".to_string())]
}

/// Fetch a conversation by its server-assigned id.
pub fn get_conversation(
    client: &Client,
    workspace_id: &str,
    conversation_id: &str,
) -> Result<Conversation> {
    client.get_data(&conversation_path(workspace_id, conversation_id), &[])
}

/// Fetch a conversation by the caller-defined `custom_id`.
///
/// Only the last path segment is reinterpreted; `workspace_id` stays a
/// server-assigned id. Delete has no such lookup, so a caller holding only a
/// `custom_id` resolves it here first.
pub fn get_conversation_by_custom_id(
    client: &Client,
    workspace_id: &str,
    custom_id: &str,
) -> Result<Conversation> {
    client.get_data(
        &conversation_path(workspace_id, custom_id),
        &by_custom_id_query(),
    )
}
