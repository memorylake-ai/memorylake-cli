//! Delete a skill (`DELETE /api/v3/skills/{id}`).

use crate::client::Client;
use crate::error::Result;

use super::skill_path;

/// Delete a skill and all of its versions.
///
/// Built-in skills cannot be deleted. Agent versions that already reference
/// the skill are not rewritten. Success carries no payload, but the envelope
/// is still checked.
pub fn delete_skill(client: &Client, id: &str) -> Result<()> {
    client.delete_empty(&skill_path(id))
}
