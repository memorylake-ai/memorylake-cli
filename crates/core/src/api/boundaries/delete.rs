//! Delete a boundary (`DELETE /api/v3/boundaries/{id}`).

use crate::client::Client;
use crate::error::Result;

use super::boundary_path;

/// Delete a boundary.
///
/// Production answers success for an id that no longer exists, so deleting
/// twice is not an error (measured 2026-10-09). Success carries no payload,
/// but the envelope is still checked.
pub fn delete_boundary(client: &Client, id: &str) -> Result<()> {
    client.delete_empty(&boundary_path(id))
}
