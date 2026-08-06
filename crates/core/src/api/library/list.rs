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
    let mut query = Vec::new();
    if let Some(page_size) = params.page_size {
        query.push(("page_size", page_size.to_string()));
    }
    if let Some(token) = &params.continuation_token {
        query.push(("continuation_token", token.clone()));
    }
    client.get_data(&children_path(item_id), &query)
}
