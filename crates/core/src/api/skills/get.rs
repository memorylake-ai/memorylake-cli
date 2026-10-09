//! Get a single skill (`GET /api/v3/skills/{id}`).

use crate::client::Client;
use crate::error::Result;

use super::skill_path;
use super::types::Skill;

/// Fetch a skill by id.
///
/// Poll this after creating a skill or publishing a version to see whether
/// the security review has finished.
pub fn get_skill(client: &Client, id: &str) -> Result<Skill> {
    client.get_data(&skill_path(id), &[])
}
