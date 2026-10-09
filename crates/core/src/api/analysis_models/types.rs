//! Shared analysis model resource types.
//!
//! Response structs capture the documented fields and collect anything else the
//! server sends via `#[serde(flatten)]`, so re-serializing a response never
//! silently drops data this client does not model.

use serde::{Deserialize, Serialize};
use serde_json::{Map, Value};

/// An analysis model: curated knowledge about one database datasource.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AnalysisModel {
    /// Server-assigned id. Opaque: do not parse it or expect a prefix.
    pub id: String,
    /// Display name.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,
    /// Free-form description.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    /// Template this model follows (e.g. `ASK_DATA`); decides which kinds of
    /// knowledge it accepts.
    #[serde(rename = "type", default, skip_serializing_if = "Option::is_none")]
    pub model_type: Option<String>,
    /// Schemas this model covers, inherited from its datasource.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub schemas: Option<Vec<String>>,
    /// Caller-defined external id.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub custom_id: Option<String>,
    /// Datasource the model is built on.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub db_datasource_id: Option<String>,
    /// Owning workspace id.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub workspace_id: Option<String>,
    /// The model this one was forked from.
    ///
    /// A record of origin, not a dependency: it keeps naming the source after
    /// that source is deleted, at which point reading it answers 404.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub fork_from: Option<String>,
    /// User or agent that created the model.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub created_by: Option<String>,
    /// Creation timestamp (ISO 8601).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub created_at: Option<String>,
    /// Last update timestamp (ISO 8601).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub updated_at: Option<String>,
    /// Fields returned by the server that this client does not model.
    #[serde(flatten)]
    pub extra: Map<String, Value>,
}

/// One kind of analysis model, and the kinds of knowledge it accepts.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AnalysisModelTemplate {
    /// Value to pass as `type` when creating a model.
    #[serde(rename = "type", default, skip_serializing_if = "Option::is_none")]
    pub model_type: Option<String>,
    /// Display name per language tag, e.g. `{"en": "...", "zh": "..."}`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub name: Option<Map<String, Value>>,
    /// Kinds of knowledge a model of this type accepts.
    #[serde(default)]
    pub entity_types: Vec<KnowledgeEntryType>,
    /// Fields returned by the server that this client does not model.
    #[serde(flatten)]
    pub extra: Map<String, Value>,
}

/// A kind of knowledge an analysis model accepts.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct KnowledgeEntryType {
    /// Value to pass as `entity_type` when adding knowledge.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub code: Option<String>,
    /// Display name per language tag.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub name: Option<Map<String, Value>>,
    /// Whether entries of this kind are matched semantically rather than only
    /// by exact lookup.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub vectorized: Option<bool>,
    /// When an entry of this kind can be deleted: `ANYWAY`, `LEAF` (nothing
    /// hangs off it) or `ISLAND` (connected to nothing at all).
    ///
    /// Kept as sent rather than parsed into an enum, so a value added
    /// server-side cannot fail decoding.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub delete_validation: Option<String>,
    /// Fields returned by the server that this client does not model.
    #[serde(flatten)]
    pub extra: Map<String, Value>,
}

/// One entry of knowledge in an analysis model.
///
/// `payload` is kept as raw JSON: its shape follows `entity_type`, the set of
/// kinds is open (a template may accept kinds the API does not describe), and
/// the CLI only prints it.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct KnowledgeEntry {
    /// Entry id.
    pub id: String,
    /// Which kind of knowledge this is; decides the shape of `payload`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub entity_type: Option<String>,
    /// The text this entry is matched against.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub embedding: Option<String>,
    /// Whether the entry is currently excluded from answering.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub disabled: Option<bool>,
    /// The entry's content; shape follows `entity_type`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub payload: Option<Value>,
    /// Whatever the caller stored alongside the entry, returned untouched.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub extra: Option<Map<String, Value>>,
    /// Closeness to a list's `keyword`, 0 to 1. Meaningless outside a search,
    /// where it is usually 0.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub score: Option<f64>,
    /// How many entries point at this one. Only returned by a single-entry get.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub source_count: Option<i64>,
    /// How many entries this one points at. Only returned by a single-entry get.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub target_count: Option<i64>,
    /// When the entry was added (ISO 8601).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub created_at: Option<String>,
    /// When it last changed (ISO 8601). Disabling or restoring counts.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub updated_at: Option<String>,
    /// Fields returned by the server that this client does not model.
    ///
    /// Not named `extra` like its siblings elsewhere: the entry has a real
    /// field of that name.
    #[serde(flatten)]
    pub unmodeled: Map<String, Value>,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn model_decodes_the_documented_shape_and_keeps_unknown_fields() {
        let raw = r#"{
            "id": "2810c96aaa8711f1b11d1e97653d06ca",
            "name": "Sales",
            "type": "ASK_DATA",
            "schemas": ["public"],
            "db_datasource_id": "ds-1",
            "workspace_id": "ws-1",
            "created_at": "2026-09-18T05:43:56.072Z",
            "brand_new_field": 7
        }"#;
        let model: AnalysisModel = serde_json::from_str(raw).expect("decode model");
        assert_eq!(model.model_type.as_deref(), Some("ASK_DATA"));
        assert_eq!(model.schemas, Some(vec!["public".to_string()]));
        assert!(model.fork_from.is_none());
        assert_eq!(model.extra["brand_new_field"], Value::from(7));

        let back = serde_json::to_value(&model).expect("encode model");
        assert_eq!(back["type"], "ASK_DATA", "the wire name must survive");
        assert_eq!(back["brand_new_field"], 7, "unknown fields must survive");
    }

    #[test]
    fn model_decodes_with_only_an_id() {
        let model: AnalysisModel = serde_json::from_str(r#"{"id":"am-1"}"#).expect("decode");
        assert_eq!(model.id, "am-1");
        assert!(model.name.is_none());
    }

    #[test]
    fn template_decodes_entity_types() {
        let raw = r#"{
            "type": "ASK_DATA",
            "name": {"en": "Ask data", "zh": "问数"},
            "entity_types": [
                {"code": "few_shot", "name": {"en": "Example"}, "vectorized": true, "delete_validation": "ANYWAY"},
                {"code": "drilldown_entity", "delete_validation": "SOMETHING_NEW"}
            ]
        }"#;
        let template: AnalysisModelTemplate = serde_json::from_str(raw).expect("decode template");
        assert_eq!(template.model_type.as_deref(), Some("ASK_DATA"));
        assert_eq!(template.entity_types.len(), 2);
        assert_eq!(template.entity_types[0].vectorized, Some(true));
        assert_eq!(
            template.entity_types[1].delete_validation.as_deref(),
            Some("SOMETHING_NEW"),
            "an unknown delete rule must not fail decoding"
        );
    }

    #[test]
    fn entry_keeps_its_own_extra_apart_from_unmodeled_fields() {
        let raw = r#"{
            "id": "f03156c2b32311f18c28b638f7ce0c3d",
            "entity_type": "few_shot",
            "disabled": false,
            "score": 0.0,
            "extra": {"owner": "analytics-team"},
            "payload": {"artifact": {"question": {"type": "TEXT", "content": "q"}}},
            "future_field": "x"
        }"#;
        let entry: KnowledgeEntry = serde_json::from_str(raw).expect("decode entry");
        assert_eq!(entry.entity_type.as_deref(), Some("few_shot"));
        assert_eq!(entry.disabled, Some(false));
        assert_eq!(
            entry.extra.as_ref().map(|extra| extra["owner"].clone()),
            Some(Value::from("analytics-team"))
        );
        assert_eq!(entry.unmodeled["future_field"], "x");

        let back = serde_json::to_value(&entry).expect("encode entry");
        assert_eq!(back["extra"]["owner"], "analytics-team");
        assert_eq!(back["future_field"], "x");
        assert_eq!(
            back["payload"]["artifact"]["question"]["content"], "q",
            "the payload must pass through untouched"
        );
    }

    #[test]
    fn entry_decodes_a_null_extra() {
        let entry: KnowledgeEntry =
            serde_json::from_str(r#"{"id":"e-1","extra":null}"#).expect("decode entry");
        assert!(entry.extra.is_none());
    }
}
