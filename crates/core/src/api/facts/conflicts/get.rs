//! Read one memory conflict (`GET .../memories/conflicts/{conflict_id}`).

use crate::client::Client;
use crate::error::Result;

use super::super::path::conflict_path;
use super::super::types::FactScope;
use super::types::MemoryConflict;

/// Fetch one conflict by id.
///
/// An id that is not shaped like a conflict id answers `INVALID_ARGUMENT`
/// ("is not a conflict id") rather than `NOT_FOUND` (measured 2026-10-09).
pub fn get_conflict(
    client: &Client,
    workspace_id: &str,
    scope: &FactScope,
    conflict_id: &str,
) -> Result<MemoryConflict> {
    client.get_data(&conflict_path(workspace_id, scope, conflict_id), &[])
}
