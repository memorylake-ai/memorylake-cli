//! Create an analysis model (`POST /api/v3/workspaces/{workspace_id}/analysis-models`).

use serde::Serialize;

use crate::client::Client;
use crate::error::Result;

use super::path::models_path;
use super::types::AnalysisModel;

/// Request body for creating an analysis model.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct CreateAnalysisModelRequest {
    /// Display name.
    pub name: String,
    /// Template to follow (e.g. `ASK_DATA`); see the templates endpoint.
    #[serde(rename = "type")]
    pub model_type: String,
    /// Datasource to build the model on. Must belong to the same workspace,
    /// and the caller needs read access to it. Immutable after creation.
    pub db_datasource_id: String,
    /// Free-form description.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    /// Caller-defined id, unique within the tenant, at most 255 characters.
    /// Immutable once set; a value starting with `_sys_` is rejected.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub custom_id: Option<String>,
    /// Existing model to start from.
    ///
    /// The source must be in this workspace with the same `type`, and must not
    /// itself be a fork. Its knowledge is copied **in the background**: the new
    /// model is empty when the call returns and fills in shortly after. Only
    /// the knowledge is inherited; name, description, datasource and type are
    /// still the caller's to supply.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub fork_from: Option<String>,
}

/// Create an analysis model in `workspace_id`.
pub fn create_analysis_model(
    client: &Client,
    workspace_id: &str,
    request: &CreateAnalysisModelRequest,
) -> Result<AnalysisModel> {
    client.post_data(&models_path(workspace_id), request)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn minimal() -> CreateAnalysisModelRequest {
        CreateAnalysisModelRequest {
            name: "Sales".into(),
            model_type: "ASK_DATA".into(),
            db_datasource_id: "ds-1".into(),
            description: None,
            custom_id: None,
            fork_from: None,
        }
    }

    #[test]
    fn omits_unset_optional_fields() {
        assert_eq!(
            serde_json::to_string(&minimal()).unwrap(),
            r#"{"name":"Sales","type":"ASK_DATA","db_datasource_id":"ds-1"}"#
        );
    }

    #[test]
    fn sends_every_optional_field_that_is_set() {
        let request = CreateAnalysisModelRequest {
            description: Some("d".into()),
            custom_id: Some("my-model".into()),
            fork_from: Some("am-src".into()),
            ..minimal()
        };
        let json = serde_json::to_value(&request).unwrap();
        assert_eq!(json["description"], "d");
        assert_eq!(json["custom_id"], "my-model");
        assert_eq!(json["fork_from"], "am-src");
    }
}
