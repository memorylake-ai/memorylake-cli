//! Get one document
//! (`GET /api/v3/workspaces/{workspace_id}/projects/{project_id}/memories/documents/{document_id}`).

use crate::client::Client;
use crate::error::Result;

use super::path::document_path;
use super::types::Document;

/// Fetch a single document from a project.
///
/// This is how import progress is observed: importing is asynchronous, so a
/// document moves through `pending` / `running` before settling on `okay` or
/// `error`.
pub fn get_document(
    client: &Client,
    workspace_id: &str,
    project_id: &str,
    document_id: &str,
) -> Result<Document> {
    client.get_data(&document_path(workspace_id, project_id, document_id), &[])
}
