//! Shared boundary resource types.

use serde::{Deserialize, Serialize};
use serde_json::{Map, Value};

/// A boundary: a saved search scope inside a workspace.
///
/// Scope fields are absent rather than empty when unset (measured
/// 2026-10-09), and the create response carries no timestamps.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Boundary {
    /// Server-assigned boundary id.
    pub id: String,
    /// Display name.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,
    /// Workspace the boundary belongs to. Fixed at creation.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub workspace_id: Option<String>,
    /// Caller-defined id, unique within the workspace.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub custom_id: Option<String>,
    /// Projects in scope; absent means no project scoping.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub project_ids: Option<Vec<String>>,
    /// Human actor in scope.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub human_actor_id: Option<String>,
    /// Agent whose memories are in scope.
    ///
    /// Production also returns that agent's `assistant_actor_id`, which the
    /// spec does not document; it is kept in `extra`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub agent_id: Option<String>,
    /// Creation timestamp (ISO 8601).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub created_at: Option<String>,
    /// User or agent that created the boundary.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub created_by: Option<String>,
    /// Last update timestamp (ISO 8601).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub updated_at: Option<String>,
    /// Fields returned by the server that this client does not model.
    #[serde(flatten)]
    pub extra: Map<String, Value>,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn live_shape_round_trips_including_undocumented_fields() {
        // Measured 2026-10-09 on a boundary scoped to all three kinds.
        let raw = serde_json::json!({
            "id": "bnd-1",
            "name": "scope",
            "workspace_id": "ws-1",
            "project_ids": ["proj-1"],
            "human_actor_id": "actor-1",
            "agent_id": "agent-1",
            "assistant_actor_id": "actor-2",
            "created_by": "user::1"
        });
        let boundary: Boundary = serde_json::from_value(raw.clone()).unwrap();
        assert_eq!(
            boundary.project_ids.as_deref(),
            Some(&["proj-1".to_string()][..])
        );
        assert!(boundary.extra.contains_key("assistant_actor_id"));
        assert_eq!(serde_json::to_value(&boundary).unwrap(), raw);
    }

    #[test]
    fn minimal_boundary_decodes() {
        let boundary: Boundary =
            serde_json::from_value(serde_json::json!({"id": "bnd-1"})).unwrap();
        assert!(boundary.project_ids.is_none());
        assert!(boundary.agent_id.is_none());
    }
}
