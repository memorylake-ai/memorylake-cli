//! Shared database-memory resource types.

use serde::{Deserialize, Serialize};
use serde_json::{Map, Value};

/// A database registered as a memory source in a project.
///
/// Every field is optional, as in the published schema; unmodeled fields are
/// kept in [`Self::extra`].
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct DatabaseMemory {
    /// Server-assigned database memory id.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub id: Option<String>,
    /// Display name.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,
    /// Guidance for answering questions against this database. Not returned
    /// when listing; get the memory to read it.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub instruction: Option<String>,
    /// Processing status, e.g. `okay` or `error`. Kept as a string so a status
    /// added server-side stays readable.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub status: Option<String>,
    /// Error detail, present only when the status is an error. Its shape is
    /// undocumented, so it is passed through whole.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub error: Option<Value>,
    /// Owning project.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub project_id: Option<String>,
    /// Datasource this memory reads.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub db_datasource_id: Option<String>,
    /// Analysis model bound to this memory; absent when none is.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub analysis_model_id: Option<String>,
    /// Creation timestamp (ISO 8601).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub created_at: Option<String>,
    /// User or agent that added this memory; absent for older entries.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub created_by: Option<String>,
    /// Fields returned by the server that this client does not model,
    /// including the deprecated `external_id`.
    #[serde(flatten)]
    pub extra: Map<String, Value>,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn documented_shape_deserializes() {
        let memory: DatabaseMemory = serde_json::from_str(
            r#"{
                "id": "db-a1b2c3d4",
                "name": "analytics_db",
                "instruction": "Revenue is in cents.",
                "status": "okay",
                "external_id": "legacy-1",
                "project_id": "proj-a1b2c3d4",
                "db_datasource_id": "ds-1",
                "analysis_model_id": "am-1",
                "created_at": "2026-04-01T00:00:00Z",
                "created_by": "user::1"
            }"#,
        )
        .expect("deserialize documented shape");

        assert_eq!(memory.status.as_deref(), Some("okay"));
        assert_eq!(memory.db_datasource_id.as_deref(), Some("ds-1"));
        // Deprecated and unmodeled, but still printed.
        assert_eq!(memory.extra["external_id"], "legacy-1");
    }

    #[test]
    fn an_errored_memory_keeps_its_error_payload() {
        let memory: DatabaseMemory = serde_json::from_str(
            r#"{"id":"db-1","status":"error","error":{"code":"SCHEMA_LOAD_FAILED"}}"#,
        )
        .unwrap();
        assert_eq!(memory.error.unwrap()["code"], "SCHEMA_LOAD_FAILED");
    }

    #[test]
    fn a_sparse_memory_still_decodes() {
        let memory: DatabaseMemory = serde_json::from_str("{}").unwrap();
        assert!(memory.id.is_none());
        assert!(memory.instruction.is_none());
    }
}
