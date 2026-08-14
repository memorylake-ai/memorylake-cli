//! List conversations
//! (`GET /api/v3/workspaces/{workspace_id}/memories/conversations`).

use serde::{Deserialize, Serialize};

use crate::client::Client;
use crate::error::Result;

use super::path::conversations_path;
use super::types::Conversation;

/// Paginated conversation list payload.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct ConversationList {
    /// Conversations on this page.
    #[serde(default)]
    pub items: Vec<Conversation>,
    /// Exact cross-page count, when the server provides it.
    #[serde(default)]
    pub total: Option<u64>,
    /// Token for the next page, if any.
    #[serde(default)]
    pub continuation_token: Option<String>,
}

/// Query parameters for listing conversations.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct ListConversationsParams {
    /// Page size. The server defaults to 20.
    pub page_size: Option<u32>,
    /// Continuation token from a previous page.
    pub continuation_token: Option<String>,
}

impl ListConversationsParams {
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

/// List the conversations inside `workspace_id`.
///
/// Paging is not performed automatically: pass the returned
/// [`continuation_token`](ConversationList::continuation_token) back to fetch
/// the next page.
pub fn list_conversations(
    client: &Client,
    workspace_id: &str,
    params: &ListConversationsParams,
) -> Result<ConversationList> {
    client.get_data(&conversations_path(workspace_id), &params.to_query())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_params_send_nothing() {
        assert!(ListConversationsParams::default().to_query().is_empty());
    }

    #[test]
    fn paging_params_render_under_their_documented_names() {
        let params = ListConversationsParams {
            page_size: Some(50),
            continuation_token: Some("tok".into()),
        };
        assert_eq!(
            params.to_query(),
            vec![
                ("page_size", "50".to_string()),
                ("continuation_token", "tok".to_string()),
            ]
        );
    }

    #[test]
    fn a_listing_page_decodes_items_total_and_token() {
        let page: ConversationList = serde_json::from_str(
            r#"{
                "items": [{"id": "conv-1", "kind": "DIRECT"}],
                "total": 2,
                "continuation_token": "tok-next"
            }"#,
        )
        .expect("decode");
        assert_eq!(page.items.len(), 1);
        assert_eq!(page.total, Some(2));
        assert_eq!(page.continuation_token.as_deref(), Some("tok-next"));
    }

    #[test]
    fn an_empty_page_decodes_with_every_field_absent() {
        let page: ConversationList = serde_json::from_str("{}").expect("decode");
        assert!(page.items.is_empty());
        assert_eq!(page.total, None);
        assert_eq!(page.continuation_token, None);
    }
}
