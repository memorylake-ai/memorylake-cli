//! Delete a Library item (`DELETE /api/v1/drives/items/{item_id}`).

use crate::client::Client;
use crate::error::Result;

use super::paths::item_path;

/// Delete a file or folder.
///
/// Deleting a folder removes every file and subfolder beneath it. The operation
/// is irreversible and the server performs no confirmation step; deleting the
/// workspace root is refused with `403 ACCESS_DENIED`.
///
/// The response carries `success` and `message` but no `data`.
pub fn delete_item(client: &Client, item_id: &str) -> Result<()> {
    client.delete_data::<()>(&item_path(item_id))
}
