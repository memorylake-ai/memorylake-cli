//! Draft a fact instruction without saving it
//! (`POST .../settings/fact-instruction/draft`).

use std::time::Duration;

use crate::client::Client;
use crate::error::Result;

use super::super::path::draft_instruction_path;
use super::super::types::FactScope;
use super::types::{DraftFactInstructionRequest, FactInstructionDraft};

/// How long to wait for a draft.
///
/// The endpoint is one synchronous language-model call. It took 2–5 seconds
/// against small scopes (measured 2026-10-09), but the API warns it runs
/// longer when the scope holds many facts — too close to the client's default
/// 30-second timeout to share it.
pub const DRAFT_TIMEOUT: Duration = Duration::from_secs(180);

/// Ask the server to write a candidate fact instruction for one scope.
///
/// Nothing is saved: review or edit the draft, then store it with
/// [`update_memory_settings`](super::update_memory_settings).
pub fn draft_fact_instruction(
    client: &Client,
    workspace_id: &str,
    scope: &FactScope,
    request: &DraftFactInstructionRequest,
) -> Result<FactInstructionDraft> {
    client.post_data_with_timeout(
        &draft_instruction_path(workspace_id, scope),
        request,
        DRAFT_TIMEOUT,
    )
}
