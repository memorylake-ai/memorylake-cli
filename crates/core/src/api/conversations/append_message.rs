//! Append a message to a conversation
//! (`POST /api/v3/conversations/{conversation_id}/messages`).

use serde::Serialize;

use crate::client::Client;
use crate::error::Result;

use super::path::messages_path;
use super::types::{ContentBlock, Message, Metadata};

/// Request body for appending a message.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct AppendMessageRequest {
    /// Actor who sent this message.
    pub actor_id: String,
    /// Caller-defined key, unique within the conversation.
    ///
    /// Required, and the reason a failed append can simply be retried: the
    /// same `custom_id` returns the message created the first time instead of
    /// appending a duplicate.
    pub custom_id: String,
    /// Ordered content blocks.
    pub content: Vec<ContentBlock>,
    /// Message this one replies to.
    ///
    /// Omitted, the message lands after the conversation's latest one. Set it
    /// to control ordering explicitly — in particular when recovering from a
    /// 409, where the correct parent is the latest message as of the retry.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub parent_message_id: Option<String>,
    /// Caller-supplied timestamp (ISO 8601); server time when omitted.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub timestamp: Option<String>,
    /// Arbitrary key/value metadata.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub metadata: Option<Metadata>,
}

/// Append a message to `conversation_id`.
///
/// The returned [`Message`] is **not** a full echo of what was stored: the
/// append response leaves `metadata`, `timestamp` and `actor_type` null even
/// when the request set them (measured 2026-08-13 against production). They
/// are stored — [`list_messages`](super::list_messages) reports them — so a
/// caller confirming what it wrote must read the listing rather than the
/// append response.
///
/// Appends within one conversation are serialized by the server: two
/// concurrent requests leave one caller with a 409 Conflict
/// ([`Error::Api`](crate::error::Error::Api) carrying that status). Recovering
/// from one means re-reading the message list and retrying against the current
/// last message — not retried here, because only the caller knows whether its
/// message still belongs at the end.
pub fn append_message(
    client: &Client,
    conversation_id: &str,
    request: &AppendMessageRequest,
) -> Result<Message> {
    client.post_data(&messages_path(conversation_id), request)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::api::conversations::types::text_block;
    use serde_json::json;

    fn minimal() -> AppendMessageRequest {
        AppendMessageRequest {
            actor_id: "actor-1".into(),
            custom_id: "msg-42".into(),
            content: vec![text_block("hello")],
            parent_message_id: None,
            timestamp: None,
            metadata: None,
        }
    }

    #[test]
    fn a_minimal_body_sends_only_the_required_fields() {
        // An omitted `parent_message_id` means "append after the latest
        // message"; sending it as null would name a message instead.
        assert_eq!(
            serde_json::to_value(minimal()).expect("serialize"),
            json!({
                "actor_id": "actor-1",
                "custom_id": "msg-42",
                "content": [{"block_type": "TEXT", "text": "hello"}]
            })
        );
    }

    #[test]
    fn every_optional_field_reaches_the_wire_under_its_documented_name() {
        let request = AppendMessageRequest {
            parent_message_id: Some("conv-entry-8".into()),
            timestamp: Some("2026-08-13T00:00:00Z".into()),
            metadata: Some(Metadata::from([("source".to_string(), "cli".to_string())])),
            ..minimal()
        };
        assert_eq!(
            serde_json::to_value(request).expect("serialize"),
            json!({
                "actor_id": "actor-1",
                "custom_id": "msg-42",
                "content": [{"block_type": "TEXT", "text": "hello"}],
                "parent_message_id": "conv-entry-8",
                "timestamp": "2026-08-13T00:00:00Z",
                "metadata": {"source": "cli"}
            })
        );
    }

    #[test]
    fn caller_supplied_blocks_are_forwarded_verbatim() {
        // Blocks the CLI does not model must survive untouched, fields and all.
        let mut block = ContentBlock::new();
        block.insert("block_type".into(), json!("TOOL_USE"));
        block.insert("tool_call_id".into(), json!("call-1"));
        block.insert("tool_name".into(), json!("search"));
        block.insert("arguments".into(), json!({"q": "revenue", "top_k": 3}));

        let request = AppendMessageRequest {
            content: vec![block],
            ..minimal()
        };
        let body = serde_json::to_value(request).expect("serialize");
        assert_eq!(body["content"][0]["arguments"]["top_k"], 3);
        assert_eq!(body["content"][0]["tool_name"], "search");
    }
}
