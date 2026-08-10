//! URL paths for the project documents endpoints.
//!
//! The resolved base URL already carries the `/openapi/memorylake` service
//! prefix, so paths here start at `/api/v3`. The published docs write the full
//! prefixed path; repeating it would produce a 404.

use crate::api::path::encode_segment;

/// Collection path for the documents held by one project.
///
/// Import, list, and delete all address this single path; only the verb and the
/// body differ.
pub(super) fn documents_path(workspace_id: &str, project_id: &str) -> String {
    format!(
        "/api/v3/workspaces/{}/projects/{}/memories/documents",
        encode_segment(workspace_id),
        encode_segment(project_id)
    )
}

/// Path to a single document inside a project.
pub(super) fn document_path(workspace_id: &str, project_id: &str, document_id: &str) -> String {
    format!(
        "{}/{}",
        documents_path(workspace_id, project_id),
        encode_segment(document_id)
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn typical_ids_stay_readable() {
        assert_eq!(
            documents_path("ws-b83fa7f09f19487f9905888f35542849", "proj-7g8h9i0j1k2l"),
            "/api/v3/workspaces/ws-b83fa7f09f19487f9905888f35542849/projects/proj-7g8h9i0j1k2l/memories/documents"
        );
        assert_eq!(
            document_path("ws-1", "proj-1", "doc-3m4n5o6p7q8r"),
            "/api/v3/workspaces/ws-1/projects/proj-1/memories/documents/doc-3m4n5o6p7q8r"
        );
    }

    #[test]
    fn every_segment_is_encoded_independently() {
        assert_eq!(
            document_path("ws a/b", "proj#c", "doc?d"),
            "/api/v3/workspaces/ws%20a%2Fb/projects/proj%23c/memories/documents/doc%3Fd"
        );
    }

    #[test]
    fn a_traversal_attempt_cannot_escape_its_segment() {
        // Without encoding this would climb back out to the workspace itself.
        assert_eq!(
            document_path("ws-1", "../..", "x"),
            "/api/v3/workspaces/ws-1/projects/..%2F../memories/documents/x"
        );
    }

    #[test]
    fn a_colon_survives_encoding() {
        // No documented document id carries one, but the shared encoder must
        // keep behaving the same way here as it does for Library item ids.
        assert_eq!(
            document_path("ws-1", "proj-1", "sc-a:inode-b"),
            "/api/v3/workspaces/ws-1/projects/proj-1/memories/documents/sc-a:inode-b"
        );
    }
}
