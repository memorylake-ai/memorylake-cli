//! Create a conversation
//! (`POST /api/v3/workspaces/{workspace_id}/memories/conversations`).

use serde::Serialize;

use crate::client::Client;
use crate::error::Result;

use super::path::conversations_path;
use super::types::{Conversation, ConversationKind, Metadata};

/// Request body for creating a conversation.
///
/// `custom_id`, `kind`, `rw_project_ids` and `actor_ids` are required by the
/// API and are therefore plain fields. Optional ones are omitted from the body
/// entirely when unset rather than sent as `null` or `[]`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct CreateConversationRequest {
    /// Caller-defined identifier, unique within the project.
    pub custom_id: String,
    /// Whether the conversation is `DIRECT` or `GROUP`.
    pub kind: ConversationKind,
    /// The project scope this conversation may read context from and write to.
    ///
    /// Bounds the conversation's access; it does not route what the server
    /// extracts. Which scope a fact is attributed to — an actor or a project —
    /// is decided server-side from the fact itself, and no request field
    /// selects it.
    ///
    /// A list on the wire, but exactly one entry is accepted for now. The
    /// caller must hold `project:mem_add` and `project:doc_add` on it.
    pub rw_project_ids: Vec<String>,
    /// Conversation title.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,
    /// Actors participating in this conversation.
    ///
    /// Required: the API rejects a create whose `actor_ids` is missing or
    /// empty, so this is always sent.
    pub actor_ids: Vec<String>,
    /// Arbitrary key/value metadata.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub metadata: Option<Metadata>,
}

/// Create a conversation inside `workspace_id`.
pub fn create_conversation(
    client: &Client,
    workspace_id: &str,
    request: &CreateConversationRequest,
) -> Result<Conversation> {
    client.post_data(&conversations_path(workspace_id), request)
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn minimal() -> CreateConversationRequest {
        CreateConversationRequest {
            custom_id: "session-42".into(),
            kind: ConversationKind::Direct,
            rw_project_ids: vec!["proj-1".into()],
            name: None,
            actor_ids: vec!["actor-1".into()],
            metadata: None,
        }
    }

    #[test]
    fn a_minimal_body_sends_only_the_required_fields() {
        // An omitted optional must be absent, not `null` and not `[]`: the API
        // reads a present-but-empty field as an explicit value. `actor_ids` is
        // not among them — the API requires it.
        assert_eq!(
            serde_json::to_value(minimal()).expect("serialize"),
            json!({
                "custom_id": "session-42",
                "kind": "DIRECT",
                "rw_project_ids": ["proj-1"],
                "actor_ids": ["actor-1"]
            })
        );
    }

    #[test]
    fn every_optional_field_reaches_the_wire_under_its_documented_name() {
        let request = CreateConversationRequest {
            name: Some("Q3 Planning".into()),
            actor_ids: vec!["actor-1".into(), "actor-2".into()],
            metadata: Some(Metadata::from([("team".to_string(), "core".to_string())])),
            kind: ConversationKind::Group,
            ..minimal()
        };
        assert_eq!(
            serde_json::to_value(request).expect("serialize"),
            json!({
                "custom_id": "session-42",
                "kind": "GROUP",
                "rw_project_ids": ["proj-1"],
                "name": "Q3 Planning",
                "actor_ids": ["actor-1", "actor-2"],
                "metadata": {"team": "core"}
            })
        );
    }
}
