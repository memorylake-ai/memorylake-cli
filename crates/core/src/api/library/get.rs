//! Get a single Library item (`GET /api/v1/drives/items/{item_id}`).

use serde::Deserialize;

use crate::client::Client;
use crate::error::Result;

use super::paths::item_path;
use super::types::Item;

/// This endpoint nests the resource one level deeper than the workspace
/// endpoints do: `data.item`, not `data`.
#[derive(Debug, Deserialize)]
struct GetItemData {
    item: Item,
}

/// Fetch a file or folder by `item_id`.
///
/// Accepts [`ROOT_ALIAS`](super::ROOT_ALIAS) in place of a concrete id.
pub fn get_item(client: &Client, item_id: &str) -> Result<Item> {
    let data: GetItemData = client.get_data(&item_path(item_id), &[])?;
    Ok(data.item)
}
