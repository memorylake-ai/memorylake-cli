//! List the messages read together with one message
//! (`GET .../memories/conversations/{id}/consumed-messages`).

use serde::{Deserialize, Serialize};
use serde_json::{Map, Value};

use crate::client::Client;
use crate::error::Result;

use super::get::by_custom_id_query;
use super::path::consumed_messages_path;

/// A message that was read as part of one batch of memory extraction.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ConsumedMessage {
    /// Message id.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub message_id: Option<String>,
    /// Caller-defined key of the message.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub custom_id: Option<String>,
    /// Position within the conversation.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub sequence_no: Option<i64>,
    /// Fields the CLI does not model yet.
    #[serde(flatten)]
    pub extra: Map<String, Value>,
}

/// List the batch of messages the server read when extracting facts from
/// `message_id`, the message itself included, in the order they were read.
///
/// An empty list means the message has not been read yet. `message_id` is
/// always an internal id; `by_custom_id` only changes how `conversation_id`
/// is read. An unknown `message_id` is answered with `500 INTERNAL_ERROR`
/// rather than a 404 (measured 2026-10-09).
pub fn list_consumed_messages(
    client: &Client,
    workspace_id: &str,
    conversation_id: &str,
    message_id: &str,
    by_custom_id: bool,
) -> Result<Vec<ConsumedMessage>> {
    let mut query = vec![("message_id", message_id.to_string())];
    if by_custom_id {
        query.extend(by_custom_id_query());
    }
    client.get_data(
        &consumed_messages_path(workspace_id, conversation_id),
        &query,
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test_support::{json_ok, one_shot_server};

    #[test]
    fn sends_the_message_and_decodes_the_bare_array() {
        let (base, server) = one_shot_server(json_ok(
            r#"{"success":true,"data":[{"message_id":"conv-entry-1","custom_id":"m1","sequence_no":1}]}"#,
        ));
        let client = Client::new(base, "sk-test").unwrap();
        let batch = list_consumed_messages(&client, "ws-1", "session-1", "conv-entry-1", true)
            .expect("list");
        assert_eq!(batch.len(), 1);
        assert_eq!(batch[0].custom_id.as_deref(), Some("m1"));
        assert_eq!(batch[0].sequence_no, Some(1));

        let request = server.join().unwrap();
        assert!(
            request.head.starts_with(
                "GET /api/v3/workspaces/ws-1/memories/conversations/session-1/consumed-messages?message_id=conv-entry-1&by_custom_id=true "
            ),
            "{}",
            request.head
        );
    }
}
