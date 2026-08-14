//! Shared conversation and message resource types.

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};
use serde_json::{Map, Value};

/// Whether a conversation is one-to-one or has several participants.
///
/// Request-side only, and therefore strict: the API rejects any other value,
/// so an unknown one must fail here rather than reach the wire. Responses
/// report the kind as a plain string ([`Conversation::kind`]) so a value added
/// server-side cannot fail the whole decode.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum ConversationKind {
    /// A conversation between two participants.
    #[serde(rename = "DIRECT")]
    Direct,
    /// A conversation with several participants.
    #[serde(rename = "GROUP")]
    Group,
}

impl ConversationKind {
    /// The wire value the API expects.
    pub fn as_wire(self) -> &'static str {
        match self {
            Self::Direct => "DIRECT",
            Self::Group => "GROUP",
        }
    }
}

/// Arbitrary caller-supplied key/value metadata.
///
/// Ordered so a serialized body is stable to assert against.
pub type Metadata = BTreeMap<String, String>;

/// One block of message content.
///
/// Held open rather than modelled as an enum over the six documented block
/// types (`TEXT`, `FILE`, `IMAGE`, `THINKING`, `TOOL_USE`, `TOOL_RESULT`).
/// Blocks are discriminated by `block_type` and carry type-specific fields;
/// a closed enum would fail to decode a whole message the day the API adds a
/// seventh, and would silently drop fields added to an existing block. The
/// CLI passes blocks through verbatim in both directions.
pub type ContentBlock = Map<String, Value>;

/// The block types the API documents.
///
/// Used to catch an obvious typo before a request is sent. Not exhaustive
/// validation — a block naming a type outside this list is still forwarded,
/// because the server is the authority on what it accepts.
pub const BLOCK_TYPES: &[&str] = &[
    "TEXT",
    "FILE",
    "IMAGE",
    "THINKING",
    "TOOL_USE",
    "TOOL_RESULT",
];

/// Build a `TEXT` content block.
pub fn text_block(text: impl Into<String>) -> ContentBlock {
    let mut block = ContentBlock::new();
    block.insert("block_type".into(), Value::String("TEXT".into()));
    block.insert("text".into(), Value::String(text.into()));
    block
}

/// One conversation.
///
/// Every field past `id` is optional so a payload that grows or omits a field
/// cannot fail the decode of an otherwise usable conversation.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Conversation {
    /// Server-assigned conversation id.
    pub id: String,
    /// Conversation title.
    #[serde(default)]
    pub name: Option<String>,
    /// `DIRECT` or `GROUP` in every documented response.
    ///
    /// Stringly typed on purpose: a kind added server-side must reach the
    /// caller unchanged rather than fail the decode.
    #[serde(default)]
    pub kind: Option<String>,
    /// Caller-defined identifier, unique within the project.
    #[serde(default)]
    pub custom_id: Option<String>,
    /// Read-write project(s) this conversation reads context from and writes
    /// memory to. Currently at most one.
    #[serde(default)]
    pub rw_project_ids: Vec<String>,
    /// Actors participating in this conversation.
    #[serde(default)]
    pub actor_ids: Vec<String>,
    /// Id of the latest message. Pass it as `parent_message_id` to append the
    /// next message at an explicit position.
    #[serde(default)]
    pub current_message_id: Option<String>,
    /// Arbitrary key/value metadata.
    #[serde(default)]
    pub metadata: Option<Metadata>,
    /// Creation timestamp (ISO 8601).
    #[serde(default)]
    pub created_at: Option<String>,
    /// Last update timestamp (ISO 8601).
    #[serde(default)]
    pub updated_at: Option<String>,
}

/// One message inside a conversation.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Message {
    /// Server-assigned message id.
    pub id: String,
    /// Ordered content blocks, passed through as received.
    #[serde(default)]
    pub content: Vec<ContentBlock>,
    /// Owning conversation id.
    #[serde(default)]
    pub conversation_id: Option<String>,
    /// Position within the conversation.
    #[serde(default)]
    pub sequence_no: Option<i64>,
    /// Actor who sent the message.
    #[serde(default)]
    pub actor_id: Option<String>,
    /// `HUMAN` or `ASSISTANT`.
    #[serde(default)]
    pub actor_type: Option<String>,
    /// Message timestamp (ISO 8601).
    #[serde(default)]
    pub timestamp: Option<String>,
    /// Arbitrary key/value metadata.
    #[serde(default)]
    pub metadata: Option<Metadata>,
    /// Creation timestamp (ISO 8601).
    #[serde(default)]
    pub created_at: Option<String>,
}

/// Whether a conversation's memory has finished building.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CookStatus {
    /// Conversation the status describes.
    #[serde(default)]
    pub conversation_id: Option<String>,
    /// True once everything appended so far has been turned into memory and is
    /// searchable.
    ///
    /// Absent is read as "not finished": a missing flag must not be reported
    /// as a finished conversation. Not every conversation reaches a finished
    /// state, so callers polling this must bound the wait themselves.
    #[serde(default)]
    pub cook_finished: bool,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn kinds_serialize_as_uppercase_wire_values() {
        assert_eq!(
            serde_json::to_string(&ConversationKind::Direct).expect("serialize"),
            r#""DIRECT""#
        );
        assert_eq!(ConversationKind::Group.as_wire(), "GROUP");
    }

    #[test]
    fn a_text_block_carries_its_discriminator() {
        let block = text_block("hello");
        assert_eq!(block["block_type"], "TEXT");
        assert_eq!(block["text"], "hello");
    }

    #[test]
    fn a_conversation_decodes_with_only_an_id() {
        let conversation: Conversation =
            serde_json::from_str(r#"{"id": "conv-a1b2"}"#).expect("decode");
        assert_eq!(conversation.id, "conv-a1b2");
        assert!(conversation.rw_project_ids.is_empty());
        assert_eq!(conversation.kind, None);
    }

    #[test]
    fn a_full_conversation_decodes_every_documented_field() {
        let conversation: Conversation = serde_json::from_str(
            r#"{
                "id": "conv-a1b2c3d4",
                "name": "Q3 Planning",
                "kind": "DIRECT",
                "custom_id": "session-42",
                "rw_project_ids": ["proj-a1b2c3d4"],
                "actor_ids": ["actor-1", "actor-2"],
                "current_message_id": "conv-entry-9",
                "metadata": {"team": "core"},
                "created_at": "2026-08-13T00:00:00Z",
                "updated_at": "2026-08-13T01:00:00Z"
            }"#,
        )
        .expect("decode");
        assert_eq!(conversation.kind.as_deref(), Some("DIRECT"));
        assert_eq!(conversation.rw_project_ids, vec!["proj-a1b2c3d4"]);
        assert_eq!(
            conversation.current_message_id.as_deref(),
            Some("conv-entry-9")
        );
        assert_eq!(
            conversation.metadata.expect("metadata")["team"],
            "core".to_string()
        );
    }

    #[test]
    fn an_unfamiliar_kind_passes_through_instead_of_failing() {
        let conversation: Conversation =
            serde_json::from_str(r#"{"id": "conv-1", "kind": "BROADCAST"}"#).expect("decode");
        assert_eq!(conversation.kind.as_deref(), Some("BROADCAST"));
    }

    #[test]
    fn unknown_fields_do_not_break_decoding() {
        let conversation: Conversation =
            serde_json::from_str(r#"{"id": "conv-1", "invented_later": {"a": [1]}}"#)
                .expect("decode");
        assert_eq!(conversation.id, "conv-1");
    }

    #[test]
    fn a_message_keeps_unknown_block_types_and_their_fields() {
        // A closed enum over the six documented block types would fail here;
        // the whole message must survive a block type added server-side.
        let message: Message = serde_json::from_str(
            r#"{
                "id": "conv-entry-1",
                "conversation_id": "conv-1",
                "sequence_no": 3,
                "actor_id": "actor-1",
                "actor_type": "HUMAN",
                "content": [
                    {"block_type": "TEXT", "text": "hi"},
                    {"block_type": "VIDEO", "uri": "s3://x", "duration_ms": 42}
                ]
            }"#,
        )
        .expect("decode");
        assert_eq!(message.sequence_no, Some(3));
        assert_eq!(message.content[1]["block_type"], "VIDEO");
        assert_eq!(message.content[1]["duration_ms"], 42);
    }

    #[test]
    fn an_absent_cook_flag_reads_as_unfinished() {
        // Reporting a missing flag as finished would tell a caller polling for
        // searchable memory to stop waiting too early.
        let status: CookStatus =
            serde_json::from_str(r#"{"conversation_id": "conv-1"}"#).expect("decode");
        assert!(!status.cook_finished);
    }
}
