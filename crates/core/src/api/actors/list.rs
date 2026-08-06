//! List actors (`GET /api/v3/actors`).

use serde::{Deserialize, Serialize};

use crate::client::Client;
use crate::error::Result;

use super::types::{Actor, ActorType};

/// Paginated actor list payload.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ActorList {
    /// Actors on this page.
    #[serde(default)]
    pub items: Vec<Actor>,
    /// Token for the next page, if any.
    #[serde(default)]
    pub continuation_token: Option<String>,
}

/// Query parameters accepted by both actor listing endpoints: the account-wide
/// `/api/v3/actors` and the workspace-scoped `/api/v3/workspaces/{id}/actors`.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct ListActorsParams {
    /// Page size. The server defaults to 20.
    pub page_size: Option<u32>,
    /// Continuation token from a previous page.
    pub continuation_token: Option<String>,
    /// Filter by actor type.
    pub actor_type: Option<ActorType>,
    /// Fuzzy filter by display name (partial match). Sent as
    /// `display_name_fuzzy`.
    pub display_name_fuzzy: Option<String>,
}

impl ListActorsParams {
    /// Render as client query pairs, omitting unset values.
    pub(super) fn to_query(&self) -> Vec<(&'static str, String)> {
        let mut query = Vec::new();
        if let Some(page_size) = self.page_size {
            query.push(("page_size", page_size.to_string()));
        }
        if let Some(token) = &self.continuation_token {
            query.push(("continuation_token", token.clone()));
        }
        if let Some(actor_type) = &self.actor_type {
            query.push(("actor_type", actor_type.as_str().to_string()));
        }
        if let Some(display_name_fuzzy) = &self.display_name_fuzzy {
            query.push(("display_name_fuzzy", display_name_fuzzy.clone()));
        }
        query
    }
}

/// List actors visible to the authenticated caller.
pub fn list_actors(client: &Client, params: &ListActorsParams) -> Result<ActorList> {
    client.get_data("/api/v3/actors", &params.to_query())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn to_query_omits_unset_params() {
        assert!(ListActorsParams::default().to_query().is_empty());
    }

    #[test]
    fn to_query_renders_every_param() {
        let params = ListActorsParams {
            page_size: Some(50),
            continuation_token: Some("token-abc".to_string()),
            actor_type: Some(ActorType::Assistant),
            display_name_fuzzy: Some("Alice".to_string()),
        };
        assert_eq!(
            params.to_query(),
            vec![
                ("page_size", "50".to_string()),
                ("continuation_token", "token-abc".to_string()),
                ("actor_type", "ASSISTANT".to_string()),
                ("display_name_fuzzy", "Alice".to_string()),
            ]
        );
    }

    #[test]
    fn actor_list_decodes_missing_continuation_token() {
        let list: ActorList = serde_json::from_str(r#"{"items":[]}"#).expect("decode empty page");
        assert!(list.items.is_empty());
        assert!(list.continuation_token.is_none());
    }
}
