//! Import Library files into a project
//! (`POST /api/v3/workspaces/{workspace_id}/projects/{project_id}/memories/documents`).

use serde::Serialize;

use crate::client::Client;
use crate::error::Result;

use super::path::documents_path;
use super::types::ImportOutcome;

/// Library files to pull into a project's document set.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct ImportDocumentsRequest {
    /// Library item ids. Each must reference an existing **file**; the server
    /// rejects a folder id, so callers expand folders themselves.
    pub drive_item_ids: Vec<String>,
}

/// Import Library files into `project_id`.
///
/// Returns as soon as the server accepts the batch. Indexing then runs
/// asynchronously, so a successful call does **not** mean the documents are
/// ready — poll [`get_document`](super::get_document) for that.
///
/// A file already imported into this project comes back as a duplicate rather
/// than an error, and a batch may partially fail: check
/// [`ImportOutcome::failure_count`] instead of relying on the call succeeding.
pub fn import_documents(
    client: &Client,
    workspace_id: &str,
    project_id: &str,
    request: &ImportDocumentsRequest,
) -> Result<ImportOutcome> {
    client.post_data(&documents_path(workspace_id, project_id), request)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn request_serializes_to_the_documented_body() {
        let body = serde_json::to_string(&ImportDocumentsRequest {
            drive_item_ids: vec!["sc-a:inode-1".to_string(), "sc-a:inode-2".to_string()],
        })
        .expect("serialize");
        assert_eq!(
            body,
            r#"{"drive_item_ids":["sc-a:inode-1","sc-a:inode-2"]}"#
        );
    }
}
