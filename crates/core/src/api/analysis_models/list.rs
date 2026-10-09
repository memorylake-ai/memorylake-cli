//! List analysis models (`GET /api/v3/workspaces/{workspace_id}/analysis-models`).

use serde::{Deserialize, Serialize};

use crate::client::Client;
use crate::error::Result;

use super::path::models_path;
use super::types::AnalysisModel;

/// Paginated analysis model list payload.
///
/// Filtering by `type` happens after the server takes a page, so a page may
/// hold fewer than `page_size` models — or none — and still carry a
/// `continuation_token`. Keep following the token until it is absent.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AnalysisModelList {
    /// Models on this page.
    #[serde(default)]
    pub items: Vec<AnalysisModel>,
    /// Total across all pages, when the server can count it. Often absent.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub total: Option<u64>,
    /// Token for the next page, if any.
    #[serde(default)]
    pub continuation_token: Option<String>,
}

/// Query parameters for listing analysis models.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct ListAnalysisModelsParams {
    /// Only models built on this datasource.
    pub db_datasource_id: Option<String>,
    /// Only models of this template type. Sent as `type`.
    pub model_type: Option<String>,
    /// Page size, 1-100. The server defaults to 20.
    pub page_size: Option<u32>,
    /// Continuation token from a previous page.
    pub continuation_token: Option<String>,
}

impl ListAnalysisModelsParams {
    /// Render as client query pairs, omitting unset values.
    fn to_query(&self) -> Vec<(&'static str, String)> {
        let mut query = Vec::new();
        if let Some(datasource) = &self.db_datasource_id {
            query.push(("db_datasource_id", datasource.clone()));
        }
        if let Some(model_type) = &self.model_type {
            query.push(("type", model_type.clone()));
        }
        if let Some(page_size) = self.page_size {
            query.push(("page_size", page_size.to_string()));
        }
        if let Some(token) = &self.continuation_token {
            query.push(("continuation_token", token.clone()));
        }
        query
    }
}

/// List the analysis models in `workspace_id` that the caller may open.
pub fn list_analysis_models(
    client: &Client,
    workspace_id: &str,
    params: &ListAnalysisModelsParams,
) -> Result<AnalysisModelList> {
    client.get_data(&models_path(workspace_id), &params.to_query())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn to_query_omits_unset_params() {
        assert!(ListAnalysisModelsParams::default().to_query().is_empty());
    }

    #[test]
    fn to_query_renders_every_param() {
        let params = ListAnalysisModelsParams {
            db_datasource_id: Some("ds-1".into()),
            model_type: Some("ASK_DATA".into()),
            page_size: Some(50),
            continuation_token: Some("tok".into()),
        };
        assert_eq!(
            params.to_query(),
            vec![
                ("db_datasource_id", "ds-1".to_string()),
                ("type", "ASK_DATA".to_string()),
                ("page_size", "50".to_string()),
                ("continuation_token", "tok".to_string()),
            ]
        );
    }

    #[test]
    fn list_decodes_a_page_without_total_or_token() {
        // What production answers for a workspace with no models.
        let list: AnalysisModelList =
            serde_json::from_str(r#"{"items":[]}"#).expect("decode empty page");
        assert!(list.items.is_empty());
        assert!(list.total.is_none());
        assert!(list.continuation_token.is_none());
    }
}
