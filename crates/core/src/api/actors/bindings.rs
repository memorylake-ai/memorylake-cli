//! Workspace/actor bindings (`/api/v3/workspaces/{id}/actors`).
//!
//! An actor exists account-wide; binding grants it participation in one
//! workspace. An actor may be bound to several workspaces at once.

use serde::{Deserialize, Serialize};

use crate::api::path::encode_segment;
use crate::client::Client;
use crate::error::Result;

use super::list::ListActorsParams;
use super::types::ActorBinding;

/// Paginated list of a workspace's actor bindings.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct WorkspaceActorList {
    /// Bindings on this page.
    #[serde(default)]
    pub items: Vec<ActorBinding>,
    /// Token for the next page, if any.
    #[serde(default)]
    pub continuation_token: Option<String>,
}

/// Request body for binding an actor to a workspace.
#[derive(Debug, Serialize)]
struct BindActorRequest<'a> {
    actor_id: &'a str,
}

fn workspace_actors_path(workspace_id: &str) -> String {
    format!("/api/v3/workspaces/{}/actors", encode_segment(workspace_id))
}

fn workspace_actor_path(workspace_id: &str, actor_id: &str) -> String {
    format!(
        "/api/v3/workspaces/{}/actors/{}",
        encode_segment(workspace_id),
        encode_segment(actor_id)
    )
}

/// Bind an existing actor to a workspace.
///
/// The actor must already exist; this endpoint does not create one.
pub fn bind_actor(client: &Client, workspace_id: &str, actor_id: &str) -> Result<ActorBinding> {
    client.post_data(
        &workspace_actors_path(workspace_id),
        &BindActorRequest { actor_id },
    )
}

/// List the actors bound to a workspace.
pub fn list_workspace_actors(
    client: &Client,
    workspace_id: &str,
    params: &ListActorsParams,
) -> Result<WorkspaceActorList> {
    client.get_data(&workspace_actors_path(workspace_id), &params.to_query())
}

/// Unbind an actor from a workspace.
///
/// The actor itself is untouched and can be rebound later. The API answers with
/// `{"success": true, "message": ...}` and no `data`.
pub fn unbind_actor(client: &Client, workspace_id: &str, actor_id: &str) -> Result<()> {
    client.delete_data(&workspace_actor_path(workspace_id, actor_id))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn workspace_actors_path_leaves_typical_ids_untouched() {
        assert_eq!(
            workspace_actors_path("ws-b83fa7f09f19487f9905888f35542849"),
            "/api/v3/workspaces/ws-b83fa7f09f19487f9905888f35542849/actors"
        );
    }

    #[test]
    fn workspace_actor_path_encodes_both_segments() {
        assert_eq!(
            workspace_actor_path("ws-1", "act-2"),
            "/api/v3/workspaces/ws-1/actors/act-2"
        );
        // Neither id may break out of its own path segment.
        assert_eq!(
            workspace_actor_path("ws/1?x", "act 2#y"),
            "/api/v3/workspaces/ws%2F1%3Fx/actors/act%202%23y"
        );
        assert_eq!(
            workspace_actor_path("100%off", "50%off"),
            "/api/v3/workspaces/100%25off/actors/50%25off"
        );
    }

    #[test]
    fn bind_request_sends_only_actor_id() {
        assert_eq!(
            serde_json::to_string(&BindActorRequest {
                actor_id: "act-a1b2c3d4e5f6"
            })
            .unwrap(),
            r#"{"actor_id":"act-a1b2c3d4e5f6"}"#
        );
    }
}
