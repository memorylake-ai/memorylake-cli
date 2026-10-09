//! List a folder's children (`GET /api/v1/drives/items/{item_id}/children`).

use serde::{Deserialize, Serialize};

use crate::client::Client;
use crate::error::Result;

use super::paths::children_path;
use super::types::Item;

/// One page of folder contents.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ItemList {
    /// Items on this page.
    #[serde(default)]
    pub items: Vec<Item>,
    /// Token for the next page, if any.
    #[serde(default)]
    pub continuation_token: Option<String>,
}

/// Query parameters for listing children.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct ListChildrenParams {
    /// Page size. The API documents a valid range of 1–50 and defaults to 50.
    pub page_size: Option<u32>,
    /// Continuation token from a previous response.
    pub continuation_token: Option<String>,
    /// Extended-attribute keys to attach to each item, sent comma-separated
    /// as `with_xattr_keys`. When given, `x_attrs` carries those keys only;
    /// `None` leaves the choice to the server, which then returns its default
    /// set (measured 2026-10-09).
    pub with_xattr_keys: Option<Vec<String>>,
}

impl ListChildrenParams {
    fn to_query(&self) -> Vec<(&'static str, String)> {
        let mut query = Vec::new();
        if let Some(page_size) = self.page_size {
            query.push(("page_size", page_size.to_string()));
        }
        if let Some(token) = &self.continuation_token {
            query.push(("continuation_token", token.clone()));
        }
        if let Some(keys) = &self.with_xattr_keys {
            query.push(("with_xattr_keys", keys.join(",")));
        }
        query
    }
}

/// List the direct children of a folder.
///
/// Accepts [`ROOT_ALIAS`](super::ROOT_ALIAS) in place of a concrete id. Paging
/// is not performed automatically: pass the returned
/// [`continuation_token`](ItemList::continuation_token) back to fetch the next
/// page.
pub fn list_children(
    client: &Client,
    item_id: &str,
    params: &ListChildrenParams,
) -> Result<ItemList> {
    client.get_data(&children_path(item_id), &params.to_query())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_params_send_nothing() {
        assert!(ListChildrenParams::default().to_query().is_empty());
    }

    #[test]
    fn xattr_keys_are_sent_comma_separated() {
        let params = ListChildrenParams {
            page_size: Some(10),
            continuation_token: None,
            with_xattr_keys: Some(vec!["team".into(), "owner".into()]),
        };
        assert_eq!(
            params.to_query(),
            vec![
                ("page_size", "10".to_string()),
                ("with_xattr_keys", "team,owner".to_string()),
            ]
        );
    }
}
