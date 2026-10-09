//! Edit a knowledge entry
//! (`PATCH /api/v3/workspaces/{workspace_id}/analysis-models/{model_id}/entries/{entry_id}`).

use serde::Serialize;
use serde_json::{Map, Value};

use crate::client::Client;
use crate::error::Result;

use super::path::entry_path;
use super::types::KnowledgeEntry;

/// Request body for editing one entry.
///
/// Only what is sent changes. `payload` and `extra` are each **replaced as a
/// whole** rather than merged: send every slot the entry should keep.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct UpdateEntryRequest {
    /// Kind of the entry being edited. Required: the server routes the edit by
    /// it and cannot always work it out from the id.
    pub entity_type: String,
    /// Replacement match text.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub embedding: Option<String>,
    /// Replacement content.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub payload: Option<Map<String, Value>>,
    /// Replacement caller data.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub extra: Option<Map<String, Value>>,
    /// `true` to exclude the entry from answering, `false` to restore it.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub disabled: Option<bool>,
}

impl UpdateEntryRequest {
    /// An edit of an entry of `entity_type` that changes nothing yet.
    pub fn new(entity_type: impl Into<String>) -> Self {
        Self {
            entity_type: entity_type.into(),
            embedding: None,
            payload: None,
            extra: None,
            disabled: None,
        }
    }

    /// `true` when nothing beyond the routing `entity_type` is set, i.e. the
    /// request would change nothing. Callers should reject this up front.
    pub fn is_empty(&self) -> bool {
        self.embedding.is_none()
            && self.payload.is_none()
            && self.extra.is_none()
            && self.disabled.is_none()
    }
}

/// Edit one knowledge entry, returning it as it stands afterwards.
pub fn update_entry(
    client: &Client,
    workspace_id: &str,
    model_id: &str,
    entry_id: &str,
    request: &UpdateEntryRequest,
) -> Result<KnowledgeEntry> {
    client.patch_data(&entry_path(workspace_id, model_id, entry_id), request)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_bare_request_is_empty_but_still_names_its_kind() {
        let request = UpdateEntryRequest::new("few_shot");
        assert!(request.is_empty());
        assert_eq!(
            serde_json::to_string(&request).unwrap(),
            r#"{"entity_type":"few_shot"}"#
        );
    }

    #[test]
    fn only_set_fields_are_sent() {
        let request = UpdateEntryRequest {
            embedding: Some("new text".into()),
            ..UpdateEntryRequest::new("general")
        };
        assert!(!request.is_empty());
        assert_eq!(
            serde_json::to_string(&request).unwrap(),
            r#"{"entity_type":"general","embedding":"new text"}"#
        );
    }

    #[test]
    fn an_empty_extra_object_is_sent_because_it_clears_the_stored_one() {
        let request = UpdateEntryRequest {
            extra: Some(Map::new()),
            ..UpdateEntryRequest::new("biz_rule")
        };
        assert!(!request.is_empty());
        assert_eq!(
            serde_json::to_string(&request).unwrap(),
            r#"{"entity_type":"biz_rule","extra":{}}"#
        );
    }
}
