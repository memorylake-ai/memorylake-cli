//! Shared project resource types.

use serde::{Deserialize, Serialize};

/// An industry classification attached to a project.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Industry {
    /// Industry identifier.
    pub id: String,
    /// Display name.
    pub name: String,
    /// Optional description.
    #[serde(default)]
    pub description: Option<String>,
}

/// A MemoryLake project: a knowledge container inside a workspace.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Project {
    /// Server-assigned project id.
    pub id: String,
    /// Display name.
    pub name: String,
    /// Optional description.
    #[serde(default)]
    pub description: Option<String>,
    /// Industry classifications, returned expanded even though they are
    /// supplied as bare `industry_ids` on create.
    #[serde(default)]
    pub industries: Vec<Industry>,
    /// Optional caller-defined metadata.
    ///
    /// Accepted on create but absent from the documented response shape; kept
    /// optional so the field is surfaced if the API starts returning it.
    #[serde(default)]
    pub metadata: Option<serde_json::Value>,
    /// Caller-defined stable external id, unique within the workspace.
    #[serde(default)]
    pub custom_id: Option<String>,
    /// Owning workspace id.
    ///
    /// Absent from the published response schema but returned by the API, so it
    /// is modelled here rather than silently dropped from command output.
    #[serde(default)]
    pub workspace_id: Option<String>,
    /// Creation timestamp (ISO 8601).
    #[serde(default)]
    pub created_at: Option<String>,
    /// Last update timestamp (ISO 8601).
    #[serde(default)]
    pub updated_at: Option<String>,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn documented_response_shape_deserializes() {
        let project: Project = serde_json::from_str(
            r#"{
                "id": "proj-1",
                "name": "Demo",
                "description": "a demo",
                "industries": [
                    {"id": "ind-1", "name": "Tech", "description": "Technology"}
                ],
                "custom_id": "demo-1",
                "created_at": "2026-01-01T00:00:00Z",
                "updated_at": "2026-01-02T00:00:00Z"
            }"#,
        )
        .expect("deserialize documented shape");

        assert_eq!(project.id, "proj-1");
        assert_eq!(project.custom_id.as_deref(), Some("demo-1"));
        assert_eq!(project.industries.len(), 1);
        assert_eq!(project.industries[0].id, "ind-1");
        assert_eq!(project.industries[0].name, "Tech");
    }

    #[test]
    fn live_response_shape_is_preserved() {
        // What the API actually returns: no timestamps, plus a `workspace_id`
        // that the published schema does not mention.
        let project: Project = serde_json::from_str(
            r#"{
                "id": "proj-1",
                "name": "probe",
                "description": "",
                "industries": [],
                "custom_id": "probe-1",
                "workspace_id": "ws-9"
            }"#,
        )
        .expect("deserialize observed shape");

        assert_eq!(project.workspace_id.as_deref(), Some("ws-9"));
        assert!(project.created_at.is_none());

        let rendered = serde_json::to_value(&project).expect("serialize");
        assert_eq!(rendered["workspace_id"], "ws-9");
    }

    #[test]
    fn optional_fields_may_be_absent() {
        // The response documentation omits `metadata` entirely, and a project
        // need not carry industries or timestamps.
        let project: Project =
            serde_json::from_str(r#"{"id":"proj-1","name":"Demo"}"#).expect("deserialize minimal");

        assert!(project.industries.is_empty());
        assert!(project.metadata.is_none());
        assert!(project.description.is_none());
        assert!(project.created_at.is_none());
    }

    #[test]
    fn industries_survive_a_serialize_round_trip() {
        // `industries` is the only expanded sub-resource; it must reach the
        // CLI's JSON output intact.
        let original: Project = serde_json::from_str(
            r#"{"id":"p","name":"n","industries":[{"id":"i","name":"Tech"}]}"#,
        )
        .expect("deserialize");
        let round_tripped: Project =
            serde_json::from_str(&serde_json::to_string(&original).expect("serialize"))
                .expect("re-deserialize");
        assert_eq!(original, round_tripped);
        assert_eq!(round_tripped.industries[0].name, "Tech");
    }
}
