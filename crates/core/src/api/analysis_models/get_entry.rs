//! Get one knowledge entry
//! (`GET /api/v3/workspaces/{workspace_id}/analysis-models/{model_id}/entries/{entry_id}`).

use crate::client::Client;
use crate::error::Result;

use super::path::entry_path;
use super::types::KnowledgeEntry;

/// Fetch one knowledge entry.
///
/// `entity_type` is optional, but some kinds cannot be located from the id
/// alone, so pass the kind the entry was created as when it is known.
pub fn get_entry(
    client: &Client,
    workspace_id: &str,
    model_id: &str,
    entry_id: &str,
    entity_type: Option<&str>,
) -> Result<KnowledgeEntry> {
    let query: Vec<(&str, String)> = entity_type
        .map(|kind| ("entity_type", kind.to_string()))
        .into_iter()
        .collect();
    client.get_data(&entry_path(workspace_id, model_id, entry_id), &query)
}
