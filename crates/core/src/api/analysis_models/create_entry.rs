//! Add a knowledge entry
//! (`POST /api/v3/workspaces/{workspace_id}/analysis-models/{model_id}/entries`).

use serde::Serialize;
use serde_json::{Map, Value};

use crate::client::Client;
use crate::error::Result;

use super::path::entries_path;
use super::types::KnowledgeEntry;

/// Request body for adding one entry of knowledge to a model.
///
/// `payload` is an open JSON object whose shape follows `entity_type` —
/// typically `{"artifact": {"<slot>": {"type": "TEXT", "desc": "...",
/// "content": "..."}}}`, with slot names fixed per kind (`question` and
/// `few_shot` for a worked example, `biz_rule` for a business rule, and so on).
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct CreateEntryRequest {
    /// Kind of knowledge (`few_shot`, `biz_rule`, `general`,
    /// `drilldown_entity`, or another kind the model's template accepts).
    pub entity_type: String,
    /// Text an incoming question is matched against.
    ///
    /// Required for every documented kind except `biz_rule`, which derives it
    /// from the rule when it is left out.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub embedding: Option<String>,
    /// The entry's content.
    pub payload: Map<String, Value>,
    /// Anything to keep alongside the entry; stored and returned untouched.
    /// A business rule on a model backed by a document library needs `doc_id`
    /// here.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub extra: Option<Map<String, Value>>,
    /// Add the entry already excluded from answering. The server defaults to
    /// `false`, and ignores it for kinds that cannot be disabled.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub disabled: Option<bool>,
}

/// Add one knowledge entry to an analysis model.
pub fn create_entry(
    client: &Client,
    workspace_id: &str,
    model_id: &str,
    request: &CreateEntryRequest,
) -> Result<KnowledgeEntry> {
    client.post_data(&entries_path(workspace_id, model_id), request)
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn payload() -> Map<String, Value> {
        let Value::Object(map) =
            json!({"artifact": {"biz_rule": {"type": "TEXT", "content": "r"}}})
        else {
            unreachable!("literal is an object")
        };
        map
    }

    #[test]
    fn omits_unset_optional_fields() {
        let request = CreateEntryRequest {
            entity_type: "biz_rule".into(),
            embedding: None,
            payload: payload(),
            extra: None,
            disabled: None,
        };
        assert_eq!(
            serde_json::to_value(&request).unwrap(),
            json!({
                "entity_type": "biz_rule",
                "payload": {"artifact": {"biz_rule": {"type": "TEXT", "content": "r"}}}
            })
        );
    }

    #[test]
    fn sends_every_field_that_is_set() {
        let mut extra = Map::new();
        extra.insert("owner".into(), Value::from("team"));
        let request = CreateEntryRequest {
            entity_type: "few_shot".into(),
            embedding: Some("q".into()),
            payload: payload(),
            extra: Some(extra),
            disabled: Some(true),
        };
        let json = serde_json::to_value(&request).unwrap();
        assert_eq!(json["embedding"], "q");
        assert_eq!(json["extra"]["owner"], "team");
        assert_eq!(json["disabled"], true);
    }
}
