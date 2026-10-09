//! Delete an analysis model
//! (`DELETE /api/v3/workspaces/{workspace_id}/analysis-models/{model_id}`).

use crate::client::Client;
use crate::error::Result;

use super::path::model_path;

/// Delete an analysis model and all of its knowledge.
///
/// Refused while any database memory still binds the model. Success carries no
/// payload (`ResponseWrapperVoid`), but the envelope is still checked.
pub fn delete_analysis_model(client: &Client, workspace_id: &str, model_id: &str) -> Result<()> {
    client.delete_empty(&model_path(workspace_id, model_id))
}
