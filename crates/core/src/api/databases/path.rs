//! URL paths for the project database-memory endpoints.

use crate::api::path::encode_segment;

/// Collection path for the database memories of one project.
pub(super) fn databases_path(workspace_id: &str, project_id: &str) -> String {
    format!(
        "/api/v3/workspaces/{}/projects/{}/memories/databases",
        encode_segment(workspace_id),
        encode_segment(project_id)
    )
}

/// Path to one database memory.
pub(super) fn database_path(workspace_id: &str, project_id: &str, database_id: &str) -> String {
    format!(
        "{}/{}",
        databases_path(workspace_id, project_id),
        encode_segment(database_id)
    )
}

/// `.../{database_id}/reload`
pub(super) fn reload_path(workspace_id: &str, project_id: &str, database_id: &str) -> String {
    format!(
        "{}/reload",
        database_path(workspace_id, project_id, database_id)
    )
}

/// `.../{database_id}/instruction/generate`
pub(super) fn generate_instruction_path(
    workspace_id: &str,
    project_id: &str,
    database_id: &str,
) -> String {
    format!(
        "{}/instruction/generate",
        database_path(workspace_id, project_id, database_id)
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn typical_ids_stay_readable() {
        assert_eq!(
            databases_path("ws-1", "proj-1"),
            "/api/v3/workspaces/ws-1/projects/proj-1/memories/databases"
        );
        assert_eq!(
            database_path("ws-1", "proj-1", "db-a1b2c3d4"),
            "/api/v3/workspaces/ws-1/projects/proj-1/memories/databases/db-a1b2c3d4"
        );
        assert_eq!(
            reload_path("ws-1", "proj-1", "db-1"),
            "/api/v3/workspaces/ws-1/projects/proj-1/memories/databases/db-1/reload"
        );
        assert_eq!(
            generate_instruction_path("ws-1", "proj-1", "db-1"),
            "/api/v3/workspaces/ws-1/projects/proj-1/memories/databases/db-1/instruction/generate"
        );
    }

    #[test]
    fn every_segment_is_encoded_independently() {
        assert_eq!(
            database_path("ws a", "proj/b", "db?c"),
            "/api/v3/workspaces/ws%20a/projects/proj%2Fb/memories/databases/db%3Fc"
        );
    }

    #[test]
    fn a_traversal_attempt_cannot_escape_its_segment() {
        // Unencoded, this would land on the project's documents instead.
        assert_eq!(
            database_path("ws-1", "proj-1", "../documents"),
            "/api/v3/workspaces/ws-1/projects/proj-1/memories/databases/..%2Fdocuments"
        );
    }
}
