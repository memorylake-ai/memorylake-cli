//! List a conversation's messages
//! (`GET /api/v3/conversations/{conversation_id}/messages`).

use serde::{Deserialize, Serialize};

use crate::client::Client;
use crate::error::Result;

use super::path::messages_path;
use super::types::Message;

/// Paginated message list payload.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct MessageList {
    /// Messages on this page.
    #[serde(default)]
    pub items: Vec<Message>,
    /// Exact cross-page count, when the server provides it.
    #[serde(default)]
    pub total: Option<u64>,
    /// Token for the next page, if any.
    #[serde(default)]
    pub continuation_token: Option<String>,
}

/// Query parameters for listing messages.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct ListMessagesParams {
    /// Page size. The server defaults to 20.
    pub page_size: Option<u32>,
    /// Continuation token from a previous page.
    pub continuation_token: Option<String>,
}

impl ListMessagesParams {
    /// Render as client query pairs, omitting unset values.
    fn to_query(&self) -> Vec<(&'static str, String)> {
        let mut query = Vec::new();
        if let Some(page_size) = self.page_size {
            query.push(("page_size", page_size.to_string()));
        }
        if let Some(token) = &self.continuation_token {
            query.push(("continuation_token", token.clone()));
        }
        query
    }
}

/// List the messages in `conversation_id`.
///
/// Takes no workspace: the messages endpoints are addressed by conversation
/// alone. Paging is not performed automatically.
pub fn list_messages(
    client: &Client,
    conversation_id: &str,
    params: &ListMessagesParams,
) -> Result<MessageList> {
    client.get_data(&messages_path(conversation_id), &params.to_query())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_params_send_nothing() {
        assert!(ListMessagesParams::default().to_query().is_empty());
    }

    #[test]
    fn paging_params_render_under_their_documented_names() {
        let params = ListMessagesParams {
            page_size: Some(20),
            continuation_token: Some("tok".into()),
        };
        assert_eq!(
            params.to_query(),
            vec![
                ("page_size", "20".to_string()),
                ("continuation_token", "tok".to_string()),
            ]
        );
    }

    #[test]
    fn a_listing_page_decodes_items_and_token() {
        let page: MessageList = serde_json::from_str(
            r#"{
                "items": [{
                    "id": "conv-entry-1",
                    "sequence_no": 1,
                    "content": [{"block_type": "TEXT", "text": "hi"}]
                }],
                "continuation_token": "tok-next"
            }"#,
        )
        .expect("decode");
        assert_eq!(page.items[0].content[0]["text"], "hi");
        assert_eq!(page.continuation_token.as_deref(), Some("tok-next"));
    }

    #[test]
    fn an_empty_page_decodes_with_every_field_absent() {
        let page: MessageList = serde_json::from_str("{}").expect("decode");
        assert!(page.items.is_empty());
        assert_eq!(page.total, None);
    }
}
