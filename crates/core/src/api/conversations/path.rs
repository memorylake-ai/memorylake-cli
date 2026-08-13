//! URL paths for the conversation endpoints.
//!
//! The resolved base URL already carries the `/openapi/memorylake` service
//! prefix, so paths here start at `/api/v3`. The published docs write the full
//! prefixed path; repeating it would produce a 404.
//!
//! Conversations are addressed two different ways and the split is deliberate,
//! not an inconsistency to smooth over: the conversation itself lives under
//! its workspace (`workspaces/{id}/memories/conversations/...`), while its
//! messages hang off the conversation alone (`conversations/{id}/messages`)
//! and take no workspace segment at all.

use crate::api::path::encode_segment;

/// Collection path for the conversations in one workspace.
///
/// Creating a conversation POSTs to this path.
pub(super) fn conversations_path(workspace_id: &str) -> String {
    format!(
        "/api/v3/workspaces/{}/memories/conversations",
        encode_segment(workspace_id)
    )
}

/// Path addressing one conversation inside its workspace.
pub(super) fn conversation_path(workspace_id: &str, conversation_id: &str) -> String {
    format!(
        "{}/{}",
        conversations_path(workspace_id),
        encode_segment(conversation_id)
    )
}

/// Path reporting one conversation's memory processing status.
pub(super) fn cook_status_path(workspace_id: &str, conversation_id: &str) -> String {
    format!(
        "{}/cook-status",
        conversation_path(workspace_id, conversation_id)
    )
}

/// Message collection path for one conversation.
///
/// Deliberately workspace-free: the messages endpoints are rooted at
/// `/api/v3/conversations`, not under the workspace tree.
pub(super) fn messages_path(conversation_id: &str) -> String {
    format!(
        "/api/v3/conversations/{}/messages",
        encode_segment(conversation_id)
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn conversations_live_under_the_workspace_memories_tree() {
        assert_eq!(
            conversations_path("ws-63ab"),
            "/api/v3/workspaces/ws-63ab/memories/conversations"
        );
    }

    #[test]
    fn one_conversation_appends_its_id() {
        assert_eq!(
            conversation_path("ws-1", "conv-a1b2"),
            "/api/v3/workspaces/ws-1/memories/conversations/conv-a1b2"
        );
    }

    #[test]
    fn cook_status_appends_the_verb() {
        assert_eq!(
            cook_status_path("ws-1", "conv-a1b2"),
            "/api/v3/workspaces/ws-1/memories/conversations/conv-a1b2/cook-status"
        );
    }

    #[test]
    fn messages_are_addressed_without_a_workspace() {
        // The messages endpoints sit at the API root, not under the workspace.
        // Building them from `conversation_path` would 404.
        assert_eq!(
            messages_path("conv-a1b2"),
            "/api/v3/conversations/conv-a1b2/messages"
        );
    }

    #[test]
    fn every_segment_is_encoded_independently() {
        assert_eq!(
            cook_status_path("ws a/b", "conv?c"),
            "/api/v3/workspaces/ws%20a%2Fb/memories/conversations/conv%3Fc/cook-status"
        );
        assert_eq!(
            messages_path("conv#d"),
            "/api/v3/conversations/conv%23d/messages"
        );
    }

    #[test]
    fn a_traversal_attempt_cannot_escape_its_segment() {
        assert_eq!(
            conversation_path("ws-1", "../.."),
            "/api/v3/workspaces/ws-1/memories/conversations/..%2F.."
        );
    }
}
