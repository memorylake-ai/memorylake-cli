//! URL construction for the project endpoints.

use crate::api::path::encode_segment;

/// Collection path for the projects owned by `workspace_id`.
pub(super) fn projects_path(workspace_id: &str) -> String {
    format!(
        "/api/v3/workspaces/{}/projects",
        encode_segment(workspace_id)
    )
}

/// Path for a single project owned by `workspace_id`.
///
/// `project_id` is the server-assigned id, or a caller-defined `custom_id` when
/// the request also sets `by_custom_id=true`.
pub(super) fn project_path(workspace_id: &str, project_id: &str) -> String {
    format!(
        "/api/v3/workspaces/{}/projects/{}",
        encode_segment(workspace_id),
        encode_segment(project_id)
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn typical_ids_stay_readable() {
        assert_eq!(
            projects_path("ws-b83fa7f09f19487f9905888f35542849"),
            "/api/v3/workspaces/ws-b83fa7f09f19487f9905888f35542849/projects"
        );
        assert_eq!(
            project_path("ws-b83fa7f09f19487f9905888f35542849", "proj_def456"),
            "/api/v3/workspaces/ws-b83fa7f09f19487f9905888f35542849/projects/proj_def456"
        );
    }

    #[test]
    fn both_segments_are_encoded_independently() {
        assert_eq!(
            project_path("ws a/b?c", "proj#d%e"),
            "/api/v3/workspaces/ws%20a%2Fb%3Fc/projects/proj%23d%25e"
        );
    }

    #[test]
    fn a_traversal_attempt_cannot_escape_its_segment() {
        // Without encoding this would address the workspace collection itself.
        assert_eq!(
            project_path("ws-1", "../.."),
            "/api/v3/workspaces/ws-1/projects/..%2F.."
        );
    }

    #[test]
    fn collection_path_encodes_the_workspace_id() {
        assert_eq!(
            projects_path("100%off"),
            "/api/v3/workspaces/100%25off/projects"
        );
    }
}
