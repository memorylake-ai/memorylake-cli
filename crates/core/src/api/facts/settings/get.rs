//! Read one scope's memory settings (`GET .../settings`).

use crate::client::Client;
use crate::error::Result;

use super::super::path::settings_path;
use super::super::types::FactScope;
use super::types::MemorySettings;

/// Fetch the memory settings of one actor, project, or agent.
pub fn get_memory_settings(
    client: &Client,
    workspace_id: &str,
    scope: &FactScope,
) -> Result<MemorySettings> {
    client.get_data(&settings_path(workspace_id, scope), &[])
}
