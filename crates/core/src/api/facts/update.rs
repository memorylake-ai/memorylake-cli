//! Edit one fact in place (`PATCH .../facts/{fact_id}`).

use serde::Serialize;
use serde_json::{Map, Value};

use crate::client::Client;
use crate::error::Result;

use super::path::fact_path;
use super::types::{Fact, FactScope};

/// Request body for editing a fact.
///
/// Both fields are optional and an absent field is left unchanged, but the
/// server rejects a body that changes nothing with `INVALID_ARGUMENT`. It also
/// treats a blank `fact` as absent rather than as new text (measured
/// 2026-10-09), so callers should reject blank text before sending it.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize)]
pub struct UpdateFactRequest {
    /// Replacement fact text.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub fact: Option<String>,
    /// Replacement metadata. **Replaces the whole stored object** rather than
    /// merging into it (documented, and measured 2026-10-09); `Some` of an
    /// empty map clears it.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub metadata: Option<Map<String, Value>>,
}

/// Edit a fact's text and/or metadata, returning the updated fact.
///
/// A text change is recorded in the fact's history as a `MANUAL` `UPDATE`
/// event (see [`trace_fact`](super::trace_fact)).
pub fn update_fact(
    client: &Client,
    workspace_id: &str,
    scope: &FactScope,
    fact_id: &str,
    request: &UpdateFactRequest,
) -> Result<Fact> {
    client.patch_data(&fact_path(workspace_id, scope, fact_id), request)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn unset_fields_are_left_out_of_the_body() {
        let text_only = UpdateFactRequest {
            fact: Some("new text".into()),
            metadata: None,
        };
        assert_eq!(
            serde_json::to_value(&text_only).expect("serialize"),
            serde_json::json!({"fact": "new text"})
        );
    }

    #[test]
    fn an_empty_metadata_object_is_sent_to_clear_it() {
        let clear = UpdateFactRequest {
            fact: None,
            metadata: Some(Map::new()),
        };
        assert_eq!(
            serde_json::to_value(&clear).expect("serialize"),
            serde_json::json!({"metadata": {}})
        );
    }
}
