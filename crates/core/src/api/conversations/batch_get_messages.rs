//! Fetch several messages by id
//! (`POST /api/v3/conversations/{conversation_id}/messages/batch-get`).

use serde::Serialize;

use crate::client::Client;
use crate::error::Result;

use super::path::batch_get_messages_path;
use super::types::Message;

/// Most message ids one batch-get accepts.
pub const BATCH_GET_MAX_MESSAGES: usize = 100;

/// Request body naming the messages to fetch.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct BatchGetMessagesRequest {
    /// Message (entry) ids, 1–[`BATCH_GET_MAX_MESSAGES`] of them. The server
    /// rejects an empty list.
    pub entry_ids: Vec<String>,
}

/// Fetch messages of one conversation by id.
///
/// Results come back in request order, and a repeated id is returned once.
/// An id the conversation does not hold fails the whole request with
/// `500 INTERNAL_ERROR` rather than being skipped (measured 2026-10-09).
pub fn batch_get_messages(
    client: &Client,
    conversation_id: &str,
    request: &BatchGetMessagesRequest,
) -> Result<Vec<Message>> {
    client.post_data(&batch_get_messages_path(conversation_id), request)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test_support::{json_ok, one_shot_server};

    #[test]
    fn posts_the_ids_and_decodes_the_bare_array() {
        let (base, server) = one_shot_server(json_ok(
            r#"{"success":true,"data":[{"id":"conv-entry-2","sequence_no":2,"agent_id":"agent-1",
                "content":[{"block_type":"TEXT","text":"hi"}]}]}"#,
        ));
        let client = Client::new(base, "sk-test").unwrap();
        let messages = batch_get_messages(
            &client,
            "conv-1",
            &BatchGetMessagesRequest {
                entry_ids: vec!["conv-entry-2".into()],
            },
        )
        .expect("batch get");
        assert_eq!(messages[0].id, "conv-entry-2");
        assert_eq!(messages[0].agent_id.as_deref(), Some("agent-1"));

        let request = server.join().unwrap();
        assert!(
            request
                .head
                .starts_with("POST /api/v3/conversations/conv-1/messages/batch-get "),
            "{}",
            request.head
        );
        assert_eq!(request.body, br#"{"entry_ids":["conv-entry-2"]}"#);
    }
}
