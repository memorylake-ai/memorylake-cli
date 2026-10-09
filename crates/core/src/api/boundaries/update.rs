//! Update a boundary (`PATCH /api/v3/boundaries/{id}`).

use serde::Serialize;

use crate::client::Client;
use crate::error::Result;

use super::boundary_path;
use super::types::Boundary;

/// Request body for updating a boundary.
///
/// Only the keys present change, so every field is skipped when `None`. The
/// "clear" spellings are values, not absences — all measured 2026-10-09:
///
/// - `project_ids: Some(vec![])` removes project scoping;
/// - `human_actor_id: Some("")` / `agent_id: Some("")` removes that scope.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize)]
pub struct UpdateBoundaryRequest {
    /// New display name (1–255 characters).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,
    /// Replacement project list. Replaces, never merges; empty clears.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub project_ids: Option<Vec<String>>,
    /// Replacement human actor; empty string clears.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub human_actor_id: Option<String>,
    /// Replacement agent; empty string clears.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub agent_id: Option<String>,
}

impl UpdateBoundaryRequest {
    /// Whether the request would change nothing.
    pub fn is_empty(&self) -> bool {
        self == &Self::default()
    }
}

/// Update a boundary. The workspace it belongs to cannot be changed.
pub fn update_boundary(
    client: &Client,
    id: &str,
    request: &UpdateBoundaryRequest,
) -> Result<Boundary> {
    client.patch_data(&boundary_path(id), request)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn omitted_fields_are_absent_from_the_body() {
        let body = serde_json::to_string(&UpdateBoundaryRequest {
            name: Some("renamed".into()),
            ..Default::default()
        })
        .unwrap();
        assert_eq!(body, r#"{"name":"renamed"}"#);
    }

    #[test]
    fn clear_spellings_are_sent_as_values() {
        let body = serde_json::to_value(UpdateBoundaryRequest {
            name: None,
            project_ids: Some(Vec::new()),
            human_actor_id: Some(String::new()),
            agent_id: Some(String::new()),
        })
        .unwrap();
        assert_eq!(
            body,
            serde_json::json!({"project_ids": [], "human_actor_id": "", "agent_id": ""})
        );
    }

    #[test]
    fn is_empty_only_when_nothing_is_set() {
        assert!(UpdateBoundaryRequest::default().is_empty());
        assert!(
            !UpdateBoundaryRequest {
                agent_id: Some(String::new()),
                ..Default::default()
            }
            .is_empty()
        );
    }
}
