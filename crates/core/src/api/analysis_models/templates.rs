//! List analysis model templates
//! (`GET /api/v3/workspaces/{workspace_id}/analysis-models/templates`).

use crate::client::Client;
use crate::error::Result;

use super::path::templates_path;
use super::types::AnalysisModelTemplate;

/// List the kinds of analysis model available and the knowledge each accepts.
///
/// Static configuration, the same for every caller; the workspace only scopes
/// the URL. `model_type` narrows the answer to that one template.
///
/// Measured 2026-10-09: production answers `500 INTERNAL_ERROR` here, with or
/// without `type`, so this call currently fails for every caller.
pub fn list_analysis_model_templates(
    client: &Client,
    workspace_id: &str,
    model_type: Option<&str>,
) -> Result<Vec<AnalysisModelTemplate>> {
    let query: Vec<(&str, String)> = model_type
        .map(|value| ("type", value.to_string()))
        .into_iter()
        .collect();
    client.get_data(&templates_path(workspace_id), &query)
}
