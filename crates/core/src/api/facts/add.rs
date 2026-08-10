//! Create facts in one scope
//! (`POST .../actors/{id}/facts` or `POST .../projects/{id}/memories/facts`).

use serde::{Deserialize, Serialize};

use crate::client::Client;
use crate::error::Result;

use super::path::facts_path;
use super::types::{Fact, FactScope};

/// Request body for creating facts.
///
/// The API takes a plain list under `facts`; the owning scope is named by the
/// request path, not the body.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct AddFactsRequest {
    /// Fact texts to store, one statement per entry.
    pub facts: Vec<String>,
}

/// The `data` payload of a fact creation.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct AddedFacts {
    /// The created facts, with their server-assigned ids.
    #[serde(default)]
    pub facts: Vec<Fact>,
}

/// Store facts under one scope.
///
/// Facts are stored verbatim — the server neither rewrites nor deduplicates
/// them (measured 2026-07-24) — and are searchable immediately, with no
/// asynchronous indexing step.
pub fn add_facts(
    client: &Client,
    workspace_id: &str,
    scope: &FactScope,
    request: &AddFactsRequest,
) -> Result<AddedFacts> {
    client.post_data(&facts_path(workspace_id, scope), request)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_body_is_a_plain_facts_list() {
        let body = serde_json::to_value(&AddFactsRequest {
            facts: vec!["a".into(), "b".into()],
        })
        .expect("serialize");
        assert_eq!(body, serde_json::json!({"facts": ["a", "b"]}));
    }

    #[test]
    fn the_creation_payload_decodes_ids_and_text() {
        // Shape measured live 2026-08-07.
        let added: AddedFacts = serde_json::from_str(
            r#"{"facts": [
                {"id": "fact-8c8a6242be3948afa8e9a970ebdb7d47", "fact": "first"},
                {"id": "fact-531bbd6f095d49a8a8f8d5809945a263", "fact": "second"}
            ]}"#,
        )
        .expect("decode");
        assert_eq!(added.facts.len(), 2);
        assert_eq!(added.facts[0].fact.as_deref(), Some("first"));
    }

    #[test]
    fn an_empty_or_absent_list_decodes_as_no_facts() {
        for raw in ["{}", r#"{"facts": []}"#] {
            let added: AddedFacts = serde_json::from_str(raw).expect("decode");
            assert!(added.facts.is_empty(), "{raw}");
        }
    }
}
