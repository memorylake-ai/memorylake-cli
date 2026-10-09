//! Get one analysis model
//! (`GET /api/v3/workspaces/{workspace_id}/analysis-models/{model_id}`).

use crate::client::Client;
use crate::error::Result;

use super::path::model_path;
use super::types::AnalysisModel;

/// Fetch an analysis model by its server-assigned id.
pub fn get_analysis_model(
    client: &Client,
    workspace_id: &str,
    model_id: &str,
) -> Result<AnalysisModel> {
    client.get_data(&model_path(workspace_id, model_id), &[])
}

/// Fetch an analysis model by its caller-defined `custom_id`.
pub fn get_analysis_model_by_custom_id(
    client: &Client,
    workspace_id: &str,
    custom_id: &str,
) -> Result<AnalysisModel> {
    client.get_data(
        &model_path(workspace_id, custom_id),
        &[("by_custom_id", "true".to_string())],
    )
}
