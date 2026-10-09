//! Update a workspace (`PATCH /api/v3/workspaces/{id}`).

use std::collections::BTreeMap;

use serde::Serialize;

use crate::client::Client;
use crate::error::Result;

use super::get::workspace_path;
use super::types::Workspace;

/// Longest workspace name the API accepts, in characters.
pub const WORKSPACE_NAME_MAX_CHARS: usize = 255;

/// Longest workspace description the API accepts, in characters.
pub const WORKSPACE_DESCRIPTION_MAX_CHARS: usize = 2000;

/// Request body for updating a workspace.
///
/// Partial update: only the fields present in the body change, so every field
/// is skipped when `None`. `metadata`, when sent, **replaces** the stored map
/// outright, and an empty map clears it (measured 2026-10-09). An empty
/// `description` clears the description, while an empty `name` is rejected.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize)]
pub struct UpdateWorkspaceRequest {
    /// New display name, 1–[`WORKSPACE_NAME_MAX_CHARS`] characters.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,
    /// New description, at most [`WORKSPACE_DESCRIPTION_MAX_CHARS`] characters.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    /// Replacement metadata map.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub metadata: Option<BTreeMap<String, String>>,
}

impl UpdateWorkspaceRequest {
    /// Whether the request would change nothing.
    ///
    /// The server answers an empty body with the unchanged workspace, which
    /// hides a caller mistake; callers reject it instead.
    pub fn is_empty(&self) -> bool {
        self.name.is_none() && self.description.is_none() && self.metadata.is_none()
    }
}

/// Update a workspace's name, description or metadata.
pub fn update_workspace(
    client: &Client,
    id: &str,
    request: &UpdateWorkspaceRequest,
) -> Result<Workspace> {
    client.patch_data(&workspace_path(id), request)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test_support::{json_ok, one_shot_server};

    #[test]
    fn unset_fields_are_absent_from_the_body() {
        let body = serde_json::to_string(&UpdateWorkspaceRequest {
            name: Some("Renamed".into()),
            ..Default::default()
        })
        .unwrap();
        assert_eq!(body, r#"{"name":"Renamed"}"#);
    }

    #[test]
    fn an_empty_metadata_map_is_sent_because_it_clears() {
        let request = UpdateWorkspaceRequest {
            metadata: Some(BTreeMap::new()),
            ..Default::default()
        };
        assert!(!request.is_empty());
        assert_eq!(
            serde_json::to_string(&request).unwrap(),
            r#"{"metadata":{}}"#
        );
    }

    #[test]
    fn an_empty_description_is_sent_verbatim() {
        let request = UpdateWorkspaceRequest {
            description: Some(String::new()),
            ..Default::default()
        };
        assert_eq!(
            serde_json::to_string(&request).unwrap(),
            r#"{"description":""}"#
        );
    }

    #[test]
    fn a_default_request_is_empty() {
        assert!(UpdateWorkspaceRequest::default().is_empty());
    }

    #[test]
    fn update_patches_the_workspace_and_keeps_unknown_fields() {
        let (base, server) = one_shot_server(json_ok(
            r#"{"success":true,"data":{"id":"ws-1","name":"n","metadata":{"team":"core"},"created_by":"user::1","new_field":7}}"#,
        ));
        let client = Client::new(base, "sk-test").unwrap();
        let metadata = BTreeMap::from([("team".to_string(), "core".to_string())]);
        let workspace = update_workspace(
            &client,
            "ws-1",
            &UpdateWorkspaceRequest {
                metadata: Some(metadata),
                ..Default::default()
            },
        )
        .expect("update");
        assert_eq!(workspace.created_by.as_deref(), Some("user::1"));
        assert_eq!(workspace.extra["new_field"], 7);

        let request = server.join().unwrap();
        assert!(
            request.head.starts_with("PATCH /api/v3/workspaces/ws-1 "),
            "{}",
            request.head
        );
        assert_eq!(request.body, br#"{"metadata":{"team":"core"}}"#);
    }
}
