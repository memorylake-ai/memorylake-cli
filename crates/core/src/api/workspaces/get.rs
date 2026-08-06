//! Get a single workspace (`GET /api/v3/workspaces/{id}`).

use crate::api::path::encode_segment;
use crate::client::Client;
use crate::error::Result;

use super::types::Workspace;

fn workspace_path(id: &str) -> String {
    format!("/api/v3/workspaces/{}", encode_segment(id))
}

/// Fetch a workspace by its server-assigned id (`ws-...`).
pub fn get_workspace(client: &Client, id: &str) -> Result<Workspace> {
    client.get_data(&workspace_path(id), &[])
}

/// Fetch a workspace by the caller-defined `custom_id`.
pub fn get_workspace_by_custom_id(client: &Client, custom_id: &str) -> Result<Workspace> {
    client.get_data(
        &workspace_path(custom_id),
        &[("by_custom_id", "true".to_string())],
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn workspace_path_leaves_typical_ids_untouched() {
        assert_eq!(
            workspace_path("ws-b83fa7f09f19487f9905888f35542849"),
            "/api/v3/workspaces/ws-b83fa7f09f19487f9905888f35542849"
        );
        assert_eq!(
            workspace_path("_sys_default_workspace"),
            "/api/v3/workspaces/_sys_default_workspace"
        );
    }

    #[test]
    fn workspace_path_escapes_url_structural_chars() {
        assert_eq!(
            workspace_path("weird id/here?foo#bar"),
            "/api/v3/workspaces/weird%20id%2Fhere%3Ffoo%23bar"
        );
    }

    #[test]
    fn workspace_path_escapes_percent_itself() {
        // A stray `%` must be encoded so it can't be misread as a pct-triplet.
        assert_eq!(workspace_path("100%off"), "/api/v3/workspaces/100%25off");
    }
}
