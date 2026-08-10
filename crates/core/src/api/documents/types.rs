//! Shared project-document resource types.

use serde::{Deserialize, Serialize};

/// `status` of a document the server has accepted but not started on.
pub const DOCUMENT_STATUS_PENDING: &str = "pending";

/// `status` of a document currently being processed.
pub const DOCUMENT_STATUS_RUNNING: &str = "running";

/// `status` of a document that finished processing successfully.
pub const DOCUMENT_STATUS_OKAY: &str = "okay";

/// `status` of a document whose processing failed.
pub const DOCUMENT_STATUS_ERROR: &str = "error";

/// `result` for a file that was imported.
pub const IMPORT_RESULT_SUCCESS: &str = "success";

/// `result` for a file that could not be imported.
pub const IMPORT_RESULT_FAILED: &str = "failed";

/// `result` for a file already present in the project.
pub const IMPORT_RESULT_DUPLICATE: &str = "duplicate";

/// Whether `status` is a state the server will not move out of.
///
/// A value this build does not recognize counts as **non-terminal**. Importing
/// is asynchronous, so a caller waiting on a document polls until this returns
/// true; treating an unfamiliar status as finished would report success for a
/// document that is still being indexed.
pub fn is_terminal_status(status: &str) -> bool {
    matches!(status, DOCUMENT_STATUS_OKAY | DOCUMENT_STATUS_ERROR)
}

/// Token accounting for one document's processing.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct DocumentUsage {
    /// Tokens spent turning the file into indexed content.
    #[serde(default)]
    pub files_process_tokens: u64,
}

/// A Library file imported into a project.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Document {
    /// Server-assigned document id. Distinct from the Library item id the file
    /// was imported from.
    pub id: String,
    /// File name, carried over from the Library item.
    pub name: String,
    /// Processing status: `pending`, `running`, `okay`, or `error`.
    ///
    /// Kept as a string rather than an enum so a status added server-side
    /// degrades to a readable value instead of failing the whole response.
    /// Use [`is_terminal_status`] rather than comparing against the known
    /// constants directly.
    pub status: String,
    /// Error detail when [`Self::status`] is [`DOCUMENT_STATUS_ERROR`].
    #[serde(default)]
    pub error: Option<serde_json::Value>,
    /// Token accounting for this document.
    #[serde(default)]
    pub usage: Option<DocumentUsage>,
    /// Source classification. `drive_file` for Library imports.
    #[serde(default)]
    pub document_type: Option<String>,
    /// Import timestamp (ISO 8601).
    #[serde(default)]
    pub created_at: Option<String>,
}

impl Document {
    /// Whether the server has finished processing this document.
    pub fn is_terminal(&self) -> bool {
        is_terminal_status(&self.status)
    }

    /// Whether processing this document failed.
    pub fn is_error(&self) -> bool {
        self.status == DOCUMENT_STATUS_ERROR
    }
}

/// What happened to one file in an import batch.
///
/// Every field is optional. Unlike a read, an import has already had its effect
/// by the time this is decoded, so a response the server changed shape on must
/// still reach the caller rather than being lost to a decode error.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct ImportDetail {
    /// `success`, `failed`, or `duplicate`.
    #[serde(default)]
    pub result: String,
    /// Library item this entry refers to.
    #[serde(default)]
    pub drive_item_id: Option<String>,
    /// Document the file became. Documented for `success`; a `duplicate` may
    /// also carry the id of the document the earlier import produced.
    #[serde(default)]
    pub document_id: Option<String>,
}

/// Summary the import endpoint returns.
///
/// The counts are authoritative. [`Self::details`] is not when
/// [`Self::details_truncated`] is set.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct ImportOutcome {
    /// Files accepted for processing.
    #[serde(default)]
    pub success_count: u32,
    /// Files that could not be imported.
    #[serde(default)]
    pub failure_count: u32,
    /// Files already present in the project.
    #[serde(default)]
    pub duplicate_count: u32,
    /// Per-file results, possibly incomplete.
    #[serde(default)]
    pub details: Vec<ImportDetail>,
    /// Whether the server omitted entries from [`Self::details`].
    ///
    /// When true, a caller that needs to act on every imported file — waiting
    /// for each to finish, say — cannot get the full set from this response.
    #[serde(default)]
    pub details_truncated: bool,
}

impl ImportOutcome {
    /// Document ids the server reported, in the order it reported them.
    ///
    /// Includes every entry carrying an id whatever its `result`: a `duplicate`
    /// points at the document an earlier import produced, which is just as
    /// valid a thing to wait on as a fresh one. Entries without an id — failures
    /// especially — have nothing to return.
    ///
    /// This is only the full set when [`Self::details_truncated`] is false.
    pub fn document_ids(&self) -> Vec<String> {
        self.details
            .iter()
            .filter_map(|detail| detail.document_id.clone())
            .collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn only_okay_and_error_are_terminal() {
        assert!(is_terminal_status(DOCUMENT_STATUS_OKAY));
        assert!(is_terminal_status(DOCUMENT_STATUS_ERROR));
        assert!(!is_terminal_status(DOCUMENT_STATUS_PENDING));
        assert!(!is_terminal_status(DOCUMENT_STATUS_RUNNING));
    }

    #[test]
    fn an_unknown_status_is_not_terminal() {
        // Reporting a document as finished because this build does not know its
        // status would be the worst possible guess.
        assert!(!is_terminal_status("reindexing"));
        assert!(!is_terminal_status(""));
    }

    #[test]
    fn documented_document_shape_deserializes() {
        let document: Document = serde_json::from_str(
            r#"{
                "id": "doc-3m4n5o6p7q8r",
                "name": "report.pdf",
                "status": "okay",
                "error": null,
                "usage": {"files_process_tokens": 1024},
                "document_type": "drive_file",
                "created_at": "2026-03-15T09:00:00Z"
            }"#,
        )
        .expect("deserialize documented shape");

        assert_eq!(document.id, "doc-3m4n5o6p7q8r");
        assert!(document.is_terminal());
        assert!(!document.is_error());
        assert_eq!(
            document.usage,
            Some(DocumentUsage {
                files_process_tokens: 1024
            })
        );
        assert_eq!(document.document_type.as_deref(), Some("drive_file"));
    }

    #[test]
    fn optional_document_fields_may_be_absent() {
        let document: Document =
            serde_json::from_str(r#"{"id":"doc-1","name":"a.pdf","status":"pending"}"#)
                .expect("deserialize minimal shape");

        assert!(!document.is_terminal());
        assert!(document.error.is_none());
        assert!(document.usage.is_none());
        assert!(document.created_at.is_none());
    }

    #[test]
    fn an_errored_document_keeps_its_error_payload() {
        let document: Document = serde_json::from_str(
            r#"{"id":"doc-1","name":"a.pdf","status":"error",
                "error":{"code":"UNSUPPORTED_FORMAT","message":"cannot parse"}}"#,
        )
        .expect("deserialize error shape");

        assert!(document.is_terminal());
        assert!(document.is_error());
        // The shape of `error` is undocumented, so it is passed through whole
        // rather than parsed into fields that may not exist.
        assert_eq!(
            document.error.as_ref().unwrap()["code"],
            "UNSUPPORTED_FORMAT"
        );
    }

    #[test]
    fn a_status_this_build_does_not_know_round_trips() {
        let document: Document =
            serde_json::from_str(r#"{"id":"doc-1","name":"a.pdf","status":"reindexing"}"#)
                .expect("an unfamiliar status must not fail the response");
        assert_eq!(document.status, "reindexing");
        assert!(!document.is_terminal());
        assert_eq!(
            serde_json::to_value(&document).unwrap()["status"],
            "reindexing"
        );
    }

    #[test]
    fn documented_import_outcome_deserializes() {
        let outcome: ImportOutcome = serde_json::from_str(
            r#"{
                "success_count": 2,
                "failure_count": 1,
                "duplicate_count": 1,
                "details": [
                    {"result": "success", "drive_item_id": "sc-a:inode-1", "document_id": "doc-1"},
                    {"result": "success", "drive_item_id": "sc-a:inode-2", "document_id": "doc-2"},
                    {"result": "duplicate", "drive_item_id": "sc-a:inode-3", "document_id": "doc-3"},
                    {"result": "failed", "drive_item_id": "sc-a:inode-4"}
                ],
                "details_truncated": false
            }"#,
        )
        .expect("deserialize documented shape");

        assert_eq!(outcome.success_count, 2);
        assert_eq!(outcome.failure_count, 1);
        assert_eq!(outcome.duplicate_count, 1);
        assert!(!outcome.details_truncated);
        // The duplicate's id is included: it names a real document that may
        // still be processing from the earlier import.
        assert_eq!(
            outcome.document_ids(),
            vec![
                "doc-1".to_string(),
                "doc-2".to_string(),
                "doc-3".to_string()
            ]
        );
    }

    #[test]
    fn an_import_response_missing_every_optional_field_still_decodes() {
        // The import already happened by the time this is parsed; losing the
        // outcome to a decode error would be worse than a sparse summary.
        let outcome: ImportOutcome =
            serde_json::from_str("{}").expect("a bare object must still decode");
        assert_eq!(outcome.success_count, 0);
        assert!(outcome.details.is_empty());
        assert!(!outcome.details_truncated);
        assert!(outcome.document_ids().is_empty());
    }

    #[test]
    fn truncated_details_are_visible_to_the_caller() {
        let outcome: ImportOutcome = serde_json::from_str(
            r#"{"success_count":900,"failure_count":0,"duplicate_count":0,
                "details":[],"details_truncated":true}"#,
        )
        .expect("deserialize truncated shape");

        assert!(outcome.details_truncated);
        // 900 files imported, no ids to wait on. The caller has to notice.
        assert_eq!(outcome.success_count, 900);
        assert!(outcome.document_ids().is_empty());
    }
}
