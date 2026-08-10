//! Remove documents from a project
//! (`DELETE /api/v3/workspaces/{workspace_id}/projects/{project_id}/memories/documents`).

use serde::Serialize;

use crate::client::Client;
use crate::error::Result;

use super::path::documents_path;

/// Documents to remove from a project.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct DeleteDocumentsRequest {
    /// Document ids to remove. These are `doc-...` ids, not the Library item
    /// ids the documents were imported from.
    pub ids: Vec<String>,
}

/// Permanently remove documents from a project.
///
/// Irreversible: the indexed content and every memory derived from these
/// documents is destroyed. The Library files they were imported from are left
/// untouched, so the same files can be imported again afterwards.
///
/// The targets travel in the request body rather than the path, which is why
/// this goes through [`Client::delete_empty_with_body`] instead of the
/// path-only delete helpers. The endpoint answers `{"success":true,"data":{}}`
/// and the empty payload is discarded; the envelope is still fully validated.
pub fn delete_documents(
    client: &Client,
    workspace_id: &str,
    project_id: &str,
    request: &DeleteDocumentsRequest,
) -> Result<()> {
    client.delete_empty_with_body(&documents_path(workspace_id, project_id), request)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn request_serializes_to_the_documented_body() {
        let body = serde_json::to_string(&DeleteDocumentsRequest {
            ids: vec![
                "doc-3m4n5o6p7q8r".to_string(),
                "doc-8s9t0u1v2w3x".to_string(),
            ],
        })
        .expect("serialize");
        assert_eq!(body, r#"{"ids":["doc-3m4n5o6p7q8r","doc-8s9t0u1v2w3x"]}"#);
    }
}
