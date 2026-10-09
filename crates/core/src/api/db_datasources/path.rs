//! URL paths for the datasource endpoints.

use crate::api::path::encode_segment;

/// Collection path for the datasources in one workspace.
pub(super) fn db_datasources_path(workspace_id: &str) -> String {
    format!(
        "/api/v3/workspaces/{}/db-datasources",
        encode_segment(workspace_id)
    )
}

/// Path to one datasource. `datasource_id` may be a `custom_id` when the
/// request also sets `by_custom_id=true`.
pub(super) fn db_datasource_path(workspace_id: &str, datasource_id: &str) -> String {
    format!(
        "{}/{}",
        db_datasources_path(workspace_id),
        encode_segment(datasource_id)
    )
}

/// Path to an action or sub-collection of one datasource (`build`, `tables`,
/// `columns`, `schema-metadata`).
pub(super) fn db_datasource_child_path(
    workspace_id: &str,
    datasource_id: &str,
    child: &str,
) -> String {
    format!(
        "{}/{child}",
        db_datasource_path(workspace_id, datasource_id)
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn typical_ids_stay_readable() {
        assert_eq!(
            db_datasources_path("ws-b83fa7f09f19487f9905888f35542849"),
            "/api/v3/workspaces/ws-b83fa7f09f19487f9905888f35542849/db-datasources"
        );
        assert_eq!(
            db_datasource_path("ws-1", "2810c96aaa8711f1b11d1e97653d06ca"),
            "/api/v3/workspaces/ws-1/db-datasources/2810c96aaa8711f1b11d1e97653d06ca"
        );
        for child in ["build", "tables", "columns", "schema-metadata"] {
            assert_eq!(
                db_datasource_child_path("ws-1", "ds-1", child),
                format!("/api/v3/workspaces/ws-1/db-datasources/ds-1/{child}")
            );
        }
    }

    #[test]
    fn every_segment_is_encoded_independently() {
        assert_eq!(
            db_datasource_child_path("ws a/b", "ds?c#d", "tables"),
            "/api/v3/workspaces/ws%20a%2Fb/db-datasources/ds%3Fc%23d/tables"
        );
    }

    #[test]
    fn a_traversal_attempt_cannot_escape_its_segment() {
        assert_eq!(
            db_datasource_path("ws-1", "../../projects"),
            "/api/v3/workspaces/ws-1/db-datasources/..%2F..%2Fprojects"
        );
    }
}
