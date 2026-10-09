//! Get one skill version (`GET /api/v3/skills/{id}/versions/{version}`).

use crate::client::Client;
use crate::error::Result;

use super::skill_version_path;
use super::types::SkillVersion;

/// Fetch a specific published version of a skill.
pub fn get_skill_version(client: &Client, id: &str, version: u64) -> Result<SkillVersion> {
    client.get_data(&skill_version_path(id, version), &[])
}
