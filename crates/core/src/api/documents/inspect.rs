//! Low-level processing details for documents
//! (`POST .../projects/{project_id}/memories/documents/inspect`).

use serde::{Deserialize, Serialize};
use serde_json::{Map, Value};

use crate::client::Client;
use crate::error::Result;

use super::path::inspect_documents_path;

/// Most document ids one inspect call accepts.
pub const INSPECT_MAX_DOCUMENTS: usize = 100;

/// Request body naming the documents to inspect.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct InspectDocumentsRequest {
    /// Document ids (`doc-...`), 1–[`INSPECT_MAX_DOCUMENTS`] of them. The
    /// server rejects an empty list.
    pub document_ids: Vec<String>,
}

/// What processing produced for one document.
///
/// Only the fields every content type shares are modelled. The rest depends
/// on `content_type` (worksheets for spreadsheets, an `llm_artifact` for
/// parsed documents, …) and is kept verbatim in `extra`. Fields ending in
/// `_s3_url` / `_s3_uri`, and `persist_path`, are pre-signed storage URLs:
/// working credentials until they expire. The client redacts them from trace
/// logs; whoever prints this value is handing them out.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct InspectedDocument {
    /// Document id.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub document_id: Option<String>,
    /// Display name.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,
    /// `pending`, `running`, `okay` or `error`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub status: Option<String>,
    /// Payload discriminator: `pdf`, `msword`, `ppt`, `excel`, `workbook`,
    /// `txt`, `image` or `nl2sql`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub content_type: Option<String>,
    /// Owning project.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub project_id: Option<String>,
    /// Ingest pipeline that processed the document (e.g. `txt_file`).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub source_type: Option<String>,
    /// Content-type-specific details.
    #[serde(flatten)]
    pub extra: Map<String, Value>,
}

/// The inspect response.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct DocumentInspection {
    /// Inspected documents.
    #[serde(default)]
    pub items: Vec<InspectedDocument>,
}

/// Inspect documents of one project.
///
/// The spec promises one item per requested id, in request order. In
/// production a document whose status is `error` is silently left out, so the
/// list can be shorter than the request (measured 2026-10-09); match items by
/// `document_id`, not by position. An unknown id fails the whole call with
/// 404.
pub fn inspect_documents(
    client: &Client,
    workspace_id: &str,
    project_id: &str,
    request: &InspectDocumentsRequest,
) -> Result<DocumentInspection> {
    client.post_data(&inspect_documents_path(workspace_id, project_id), request)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn request_serializes_to_the_documented_body() {
        let body = serde_json::to_string(&InspectDocumentsRequest {
            document_ids: vec!["doc-1".into(), "doc-2".into()],
        })
        .unwrap();
        assert_eq!(body, r#"{"document_ids":["doc-1","doc-2"]}"#);
    }

    #[test]
    fn content_specific_fields_survive_a_round_trip() {
        let inspection: DocumentInspection = serde_json::from_str(
            r#"{"items":[{"name":"probe.txt","status":"okay","summary":"s","content_type":"txt",
                "project_id":"proj-1","document_id":"doc-1","source_type":"txt_file",
                "llm_artifact":{"index_md_s3_uri":"https://s3/x"}}]}"#,
        )
        .expect("decode");
        let item = &inspection.items[0];
        assert_eq!(item.document_id.as_deref(), Some("doc-1"));
        assert_eq!(item.content_type.as_deref(), Some("txt"));
        assert_eq!(item.extra["summary"], "s");

        let reprinted = serde_json::to_value(&inspection).unwrap();
        assert_eq!(
            reprinted["items"][0]["llm_artifact"]["index_md_s3_uri"],
            "https://s3/x"
        );
    }

    #[test]
    fn an_empty_response_decodes() {
        let inspection: DocumentInspection = serde_json::from_str("{}").expect("decode");
        assert!(inspection.items.is_empty());
    }
}
