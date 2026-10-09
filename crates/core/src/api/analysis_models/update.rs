//! Update an analysis model
//! (`PATCH /api/v3/workspaces/{workspace_id}/analysis-models/{model_id}`).

use serde::Serialize;

use crate::client::Client;
use crate::error::Result;

use super::path::model_path;
use super::types::AnalysisModel;

/// Request body for a partial analysis model update.
///
/// Only name and description can change; type, datasource, custom id and fork
/// origin are fixed at creation. Unset fields are left out of the body, which
/// the server reads as "keep the current value".
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize)]
pub struct UpdateAnalysisModelRequest {
    /// New display name.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,
    /// New description.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
}

impl UpdateAnalysisModelRequest {
    /// `true` when no field is set, i.e. the request would change nothing.
    /// Callers should reject this before making a request.
    pub fn is_empty(&self) -> bool {
        self.name.is_none() && self.description.is_none()
    }
}

/// Apply a partial update to an analysis model.
pub fn update_analysis_model(
    client: &Client,
    workspace_id: &str,
    model_id: &str,
    request: &UpdateAnalysisModelRequest,
) -> Result<AnalysisModel> {
    client.patch_data(&model_path(workspace_id, model_id), request)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_request_is_empty() {
        assert!(UpdateAnalysisModelRequest::default().is_empty());
        assert_eq!(
            serde_json::to_string(&UpdateAnalysisModelRequest::default()).unwrap(),
            "{}"
        );
    }

    #[test]
    fn only_set_fields_are_sent() {
        let request = UpdateAnalysisModelRequest {
            description: Some(String::new()),
            ..UpdateAnalysisModelRequest::default()
        };
        assert!(!request.is_empty());
        assert_eq!(
            serde_json::to_string(&request).unwrap(),
            r#"{"description":""}"#
        );
    }
}
