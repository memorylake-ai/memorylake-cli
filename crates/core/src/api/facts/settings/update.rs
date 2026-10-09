//! Change one scope's memory settings (`PATCH .../settings`).

use crate::client::Client;
use crate::error::Result;

use super::super::path::settings_path;
use super::super::types::FactScope;
use super::types::{MemorySettings, UpdateMemorySettingsRequest};

/// Apply a partial settings update and return the settings now in effect.
///
/// An instruction longer than
/// [`MAX_FACT_INSTRUCTION_LEN`](super::MAX_FACT_INSTRUCTION_LEN) answers
/// `INVALID_ARGUMENT`. Whitespace is stored as given, so a blank but
/// non-empty instruction does not restore the default (measured 2026-10-09).
pub fn update_memory_settings(
    client: &Client,
    workspace_id: &str,
    scope: &FactScope,
    request: &UpdateMemorySettingsRequest,
) -> Result<MemorySettings> {
    client.patch_data(&settings_path(workspace_id, scope), request)
}
