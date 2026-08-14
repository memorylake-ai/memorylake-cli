//! Read a conversation's memory processing status
//! (`GET .../memories/conversations/{id}/cook-status`).

use crate::client::Client;
use crate::error::Result;

use super::get::by_custom_id_query;
use super::path::cook_status_path;
use super::types::CookStatus;

/// Report whether `conversation_id`'s memory has finished building.
///
/// Memory is built asynchronously after messages are appended, so a
/// conversation is not immediately searchable. Callers poll this — but not
/// indefinitely: not every conversation reaches a finished state, so the wait
/// needs a caller-side timeout.
pub fn get_cook_status(
    client: &Client,
    workspace_id: &str,
    conversation_id: &str,
) -> Result<CookStatus> {
    client.get_data(&cook_status_path(workspace_id, conversation_id), &[])
}

/// Same, addressing the conversation by its caller-defined `custom_id`.
///
/// `workspace_id` stays a server-assigned id.
pub fn get_cook_status_by_custom_id(
    client: &Client,
    workspace_id: &str,
    custom_id: &str,
) -> Result<CookStatus> {
    client.get_data(
        &cook_status_path(workspace_id, custom_id),
        &by_custom_id_query(),
    )
}
