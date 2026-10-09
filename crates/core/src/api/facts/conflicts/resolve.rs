//! Resolve one memory conflict
//! (`POST .../memories/conflicts/{conflict_id}/resolve`).

use serde::{Deserialize, Serialize, Serializer};
use serde_json::{Map, Value};

use crate::client::Client;
use crate::error::Result;

use super::super::path::resolve_conflict_path;
use super::super::types::FactScope;
use super::types::ConflictFactEdit;

/// How to resolve a conflict: the strategy together with the inputs only that
/// strategy takes.
///
/// The wire body is flat (`strategy`, `keep_fact_id`, `edits`), but an input
/// paired with the wrong strategy is not harmless: `dismiss` with a stray
/// `keep_fact_id` answered `INTERNAL_ERROR` (measured 2026-10-09). Carrying
/// each input inside its own variant makes that body impossible to build.
///
/// Which strategies a conflict accepts depends on its category, and the server
/// enforces it (`INVALID_ARGUMENT` otherwise):
///
/// | category | strategies |
/// |---|---|
/// | `m2m` (fact vs fact) | `keep_fact`, `dismiss` |
/// | `m2d` (fact vs document) | `trust_fact`, `trust_document`, `dismiss` |
/// | `self` (one fact) | `edit_fact`, `dismiss` |
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ConflictResolution {
    /// Keep this fact and forget the other one. It must be one of the
    /// conflict's facts.
    KeepFact {
        /// The fact to keep.
        keep_fact_id: String,
    },
    /// Settle a fact-vs-document conflict in the fact's favor; the server
    /// generates the rewritten wording.
    TrustFact,
    /// Settle a fact-vs-document conflict in the document's favor.
    TrustDocument,
    /// Mark the conflict a false alarm; no fact changes.
    Dismiss,
    /// Replace the text of the conflict's facts. Each fact may appear at most
    /// once and at least one edit is required; facts left out keep their text.
    EditFact {
        /// Replacement text per fact.
        edits: Vec<ConflictFactEdit>,
    },
}

impl ConflictResolution {
    /// The wire name of this strategy.
    pub fn strategy(&self) -> &'static str {
        match self {
            Self::KeepFact { .. } => "keep_fact",
            Self::TrustFact => "trust_fact",
            Self::TrustDocument => "trust_document",
            Self::Dismiss => "dismiss",
            Self::EditFact { .. } => "edit_fact",
        }
    }
}

impl Serialize for ConflictResolution {
    fn serialize<S: Serializer>(&self, serializer: S) -> std::result::Result<S::Ok, S::Error> {
        #[derive(Serialize)]
        struct Wire<'a> {
            strategy: &'static str,
            #[serde(skip_serializing_if = "Option::is_none")]
            keep_fact_id: Option<&'a str>,
            #[serde(skip_serializing_if = "Option::is_none")]
            edits: Option<&'a [ConflictFactEdit]>,
        }

        let (keep_fact_id, edits) = match self {
            Self::KeepFact { keep_fact_id } => (Some(keep_fact_id.as_str()), None),
            Self::EditFact { edits } => (None, Some(edits.as_slice())),
            Self::TrustFact | Self::TrustDocument | Self::Dismiss => (None, None),
        };
        Wire {
            strategy: self.strategy(),
            keep_fact_id,
            edits,
        }
        .serialize(serializer)
    }
}

/// The `data` payload of a resolution.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ConflictResolveResult {
    /// Conflict id.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub id: Option<String>,
    /// Whether the conflict is now resolved.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub resolved: Option<bool>,
    /// Strategy applied.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub strategy: Option<String>,
    /// Id of the stored resolution record (`cflr-...`).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub resolve_id: Option<String>,
    /// Facts forgotten by the resolution.
    #[serde(default)]
    pub forgotten_fact_ids: Vec<String>,
    /// Facts rewritten by the resolution.
    #[serde(default)]
    pub updated_fact_ids: Vec<String>,
    /// Fields not modeled above, preserved for output.
    #[serde(flatten)]
    pub extra: Map<String, Value>,
}

/// Resolve one conflict.
///
/// A conflict resolves once: resolving it again answers `INVALID_ARGUMENT`
/// ("already resolved", measured 2026-10-09).
pub fn resolve_conflict(
    client: &Client,
    workspace_id: &str,
    scope: &FactScope,
    conflict_id: &str,
    resolution: &ConflictResolution,
) -> Result<ConflictResolveResult> {
    client.post_data(
        &resolve_conflict_path(workspace_id, scope, conflict_id),
        resolution,
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    fn body(resolution: &ConflictResolution) -> Value {
        serde_json::to_value(resolution).expect("serialize")
    }

    #[test]
    fn strategies_without_inputs_send_only_the_strategy() {
        assert_eq!(
            body(&ConflictResolution::Dismiss),
            serde_json::json!({"strategy": "dismiss"})
        );
        assert_eq!(
            body(&ConflictResolution::TrustFact),
            serde_json::json!({"strategy": "trust_fact"})
        );
        assert_eq!(
            body(&ConflictResolution::TrustDocument),
            serde_json::json!({"strategy": "trust_document"})
        );
    }

    #[test]
    fn keep_fact_carries_only_its_fact_id() {
        assert_eq!(
            body(&ConflictResolution::KeepFact {
                keep_fact_id: "fact-ec7a".into()
            }),
            serde_json::json!({"strategy": "keep_fact", "keep_fact_id": "fact-ec7a"})
        );
    }

    #[test]
    fn edit_fact_carries_only_its_edits() {
        assert_eq!(
            body(&ConflictResolution::EditFact {
                edits: vec![ConflictFactEdit {
                    fact_id: "fact-f12e".into(),
                    new_fact_text: "Always answer in French.".into(),
                }]
            }),
            serde_json::json!({
                "strategy": "edit_fact",
                "edits": [{"fact_id": "fact-f12e", "new_fact_text": "Always answer in French."}]
            })
        );
    }

    #[test]
    fn a_measured_result_round_trips_unchanged() {
        // Shape measured live 2026-10-09 for `keep_fact`.
        let raw = serde_json::json!({
            "id": "cfl-1870",
            "resolved": true,
            "strategy": "keep_fact",
            "resolve_id": "cflr-3f59",
            "forgotten_fact_ids": ["fact-a23c"],
            "updated_fact_ids": []
        });
        let result: ConflictResolveResult = serde_json::from_value(raw.clone()).expect("decode");
        assert_eq!(result.forgotten_fact_ids, vec!["fact-a23c".to_string()]);
        assert_eq!(serde_json::to_value(&result).expect("encode"), raw);
    }
}
