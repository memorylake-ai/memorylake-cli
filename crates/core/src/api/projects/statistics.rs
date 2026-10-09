//! Project statistics
//! (`GET /api/v3/workspaces/{workspace_id}/projects/{project_id}/statistics`).

use serde::{Deserialize, Serialize};
use serde_json::{Map, Value};

use crate::client::Client;
use crate::error::Result;

use super::path::project_statistics_path;

/// How many of a project's documents or databases sit in each processing
/// stage, in the same vocabulary as a document's own `status`.
///
/// The four figures add up to the matching count on [`ProjectStatistics`].
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct StatusSummary {
    /// Queued but not started.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub pending: Option<u64>,
    /// Being processed.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub running: Option<u64>,
    /// Finished and searchable.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub okay: Option<u64>,
    /// Finished but could not be made searchable.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub error: Option<u64>,
    /// Stages the CLI does not know yet.
    #[serde(flatten)]
    pub extra: Map<String, Value>,
}

/// Document and database counts for one project.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct ProjectStatistics {
    /// Documents in the project.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub document_count: Option<u64>,
    /// Databases in the project.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub database_count: Option<u64>,
    /// Documents per processing stage.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub document_status: Option<StatusSummary>,
    /// Databases per processing stage.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub database_status: Option<StatusSummary>,
    /// Fields the CLI does not model yet.
    #[serde(flatten)]
    pub extra: Map<String, Value>,
}

/// Fetch a project's document and database counts.
///
/// Takes the server-assigned project id only: the endpoint has no
/// `by_custom_id` switch, and a custom_id is answered with 404 (measured
/// 2026-10-09).
pub fn get_project_statistics(
    client: &Client,
    workspace_id: &str,
    project_id: &str,
) -> Result<ProjectStatistics> {
    client.get_data(&project_statistics_path(workspace_id, project_id), &[])
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_measured_response_decodes() {
        let stats: ProjectStatistics = serde_json::from_str(
            r#"{"document_count":2,"database_count":0,
                "document_status":{"pending":0,"running":0,"okay":1,"error":1},
                "database_status":{"pending":0,"running":0,"okay":0,"error":0}}"#,
        )
        .expect("decode");
        assert_eq!(stats.document_count, Some(2));
        let documents = stats.document_status.expect("document_status");
        assert_eq!(documents.okay, Some(1));
        assert_eq!(documents.error, Some(1));
    }

    #[test]
    fn an_empty_object_decodes_and_unknown_fields_survive() {
        let stats: ProjectStatistics =
            serde_json::from_str(r#"{"document_status":{"paused":3},"index_count":4}"#)
                .expect("decode");
        assert_eq!(stats.document_count, None);
        let reprinted = serde_json::to_value(&stats).unwrap();
        assert_eq!(reprinted["document_status"]["paused"], 3);
        assert_eq!(reprinted["index_count"], 4);
    }
}
