//! Create a boundary (`POST /api/v3/boundaries`).

use serde::Serialize;

use crate::client::Client;
use crate::error::Result;

use super::BOUNDARIES_PATH;
use super::types::Boundary;

/// Request body for creating a boundary.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize)]
pub struct CreateBoundaryRequest {
    /// Display name (1–255 characters).
    pub name: String,
    /// Workspace the boundary belongs to. Cannot be changed later.
    pub workspace_id: String,
    /// Caller-defined id, unique within the workspace (`CUSTOM_ID_CONFLICT`
    /// otherwise).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub custom_id: Option<String>,
    /// Projects to scope. Each must exist in the workspace.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub project_ids: Option<Vec<String>>,
    /// Human actor to scope. Must be bound to the workspace.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub human_actor_id: Option<String>,
    /// Agent whose memories are in scope. Must be bound to the workspace.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub agent_id: Option<String>,
}

/// Create a boundary.
pub fn create_boundary(client: &Client, request: &CreateBoundaryRequest) -> Result<Boundary> {
    client.post_data(BOUNDARIES_PATH, request)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn minimal_body_carries_only_the_required_fields() {
        let body = serde_json::to_value(CreateBoundaryRequest {
            name: "scope".into(),
            workspace_id: "ws-1".into(),
            ..Default::default()
        })
        .unwrap();
        assert_eq!(
            body,
            serde_json::json!({"name": "scope", "workspace_id": "ws-1"})
        );
    }

    #[test]
    fn full_body_has_the_documented_shape() {
        let body = serde_json::to_value(CreateBoundaryRequest {
            name: "scope".into(),
            workspace_id: "ws-1".into(),
            custom_id: Some("c-1".into()),
            project_ids: Some(vec!["proj-1".into(), "proj-2".into()]),
            human_actor_id: Some("actor-1".into()),
            agent_id: Some("agent-1".into()),
        })
        .unwrap();
        assert_eq!(
            body,
            serde_json::json!({
                "name": "scope",
                "workspace_id": "ws-1",
                "custom_id": "c-1",
                "project_ids": ["proj-1", "proj-2"],
                "human_actor_id": "actor-1",
                "agent_id": "agent-1"
            })
        );
    }
}
