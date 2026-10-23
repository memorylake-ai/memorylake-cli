//! Delete a Library item (`DELETE /api/v1/drives/items/{item_id}`).

use serde::Serialize;

use crate::client::Client;
use crate::error::Result;

use super::paths::{ITEMS_PATH, item_path};

/// Delete a file or folder.
///
/// Deleting a folder removes every file and subfolder beneath it. The operation
/// is irreversible and the server performs no confirmation step; deleting the
/// workspace root is refused with `403 ACCESS_DENIED`.
///
/// Documented to answer with an empty `data` object, which
/// [`Client::delete_empty`] accepts whether or not the field is present.
pub fn delete_item(client: &Client, item_id: &str) -> Result<()> {
    client.delete_empty(&item_path(item_id))
}

/// Most items the server accepts in one batch delete.
pub const MAX_BATCH_DELETE_ITEMS: usize = 100;

#[derive(Debug, Serialize)]
struct BatchDeleteRequest<'a> {
    item_ids: &'a [String],
}

/// Delete up to [`MAX_BATCH_DELETE_ITEMS`] files or folders in one request
/// (`POST /api/v1/drives/items/delete`).
///
/// Same semantics as [`delete_item`] for each entry: folders go with
/// everything beneath them, and the request is irreversible. Including the
/// workspace root rejects the whole request with `403` and deletes nothing;
/// any other failure stops the batch partway, so re-check what remains before
/// retrying.
pub fn delete_items(client: &Client, item_ids: &[String]) -> Result<()> {
    client.post_empty_with_body(
        &format!("{ITEMS_PATH}/delete"),
        &BatchDeleteRequest { item_ids },
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn batch_request_serializes_to_the_documented_body() {
        let ids = vec!["sc-a:i-1".to_string(), "sc-a:i-2".to_string()];
        let body = serde_json::to_string(&BatchDeleteRequest { item_ids: &ids }).expect("ser");
        assert_eq!(body, r#"{"item_ids":["sc-a:i-1","sc-a:i-2"]}"#);
    }
}
