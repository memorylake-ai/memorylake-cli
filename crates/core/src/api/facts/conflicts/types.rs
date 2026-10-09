//! Memory conflict resource types.
//!
//! Response structs keep enumerated values as strings and collect unmodeled
//! fields into `extra`, so a value added server-side reaches the caller
//! instead of failing the decode. Request-side values are closed enums: an
//! unknown resolution strategy makes the server answer `INTERNAL_ERROR`
//! rather than a validation error (measured 2026-10-09), so it must never be
//! sent.

use serde::{Deserialize, Serialize};
use serde_json::{Map, Value};

/// What a conflict is between, as a list filter.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ConflictCategory {
    /// Two facts contradict each other (`m2m`).
    FactVsFact,
    /// A fact contradicts a document (`m2d`). Only projects compare facts
    /// against documents, so this is always empty for actors and agents.
    FactVsDocument,
    /// One fact contradicts itself (`self`).
    SelfContradiction,
}

impl ConflictCategory {
    /// The query-string spelling.
    pub fn as_wire(self) -> &'static str {
        match self {
            Self::FactVsFact => "m2m",
            Self::FactVsDocument => "m2d",
            Self::SelfContradiction => "self",
        }
    }
}

/// Kind of inconsistency, as a list filter.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ConflictType {
    /// The statements cannot all be true. The only kind actors and agents
    /// produce.
    Logical,
    /// A statement disagrees with known material.
    Knowledge,
}

impl ConflictType {
    /// The query-string spelling.
    pub fn as_wire(self) -> &'static str {
        match self {
            Self::Logical => "logical",
            Self::Knowledge => "knowledge",
        }
    }
}

/// A replacement text for one fact involved in a conflict.
///
/// Sent in an `edit_fact` resolution; read back in a resolution record as the
/// text each rewritten fact ended up with.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ConflictFactEdit {
    /// One of the conflict's facts.
    pub fact_id: String,
    /// Replacement text. It overwrites the fact's current text even if the
    /// fact changed after the conflict was detected.
    pub new_fact_text: String,
}

/// A detected memory conflict.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct MemoryConflict {
    /// Conflict id (`cfl-...` in every observed response).
    pub id: String,
    /// Short human-readable name.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,
    /// Explanation of the contradiction.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    /// `m2m`, `m2d`, or `self` — see [`ConflictCategory`].
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub category: Option<String>,
    /// `logical` or `knowledge` — see [`ConflictType`].
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub conflict_type: Option<String>,
    /// Whether the conflict has been resolved.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub resolved: Option<bool>,
    /// Whether an involved fact changed after detection.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub stale: Option<bool>,
    /// How the conflict was resolved; present only once it has been.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub resolve: Option<ConflictResolutionRecord>,
    /// Ids of the facts involved; a single id for `m2d` and `self`.
    #[serde(default)]
    pub fact_ids: Vec<String>,
    /// The involved facts' text at detection time.
    #[serde(default)]
    pub fact_snapshots: Vec<ConflictFactSnapshot>,
    /// Document excerpts cited by a fact-vs-document conflict.
    #[serde(default)]
    pub file_chunks: Vec<ConflictFileChunk>,
    /// When the conflict was first detected (ISO 8601).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub created_at: Option<String>,
    /// When the conflict was last updated (ISO 8601).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub updated_at: Option<String>,
    /// Fields not modeled above, preserved for output.
    #[serde(flatten)]
    pub extra: Map<String, Value>,
}

/// One involved fact's text at detection time.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ConflictFactSnapshot {
    /// Fact id.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub fact_id: Option<String>,
    /// Fact text at detection time.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub fact_text: Option<String>,
    /// Fields not modeled above, preserved for output.
    #[serde(flatten)]
    pub extra: Map<String, Value>,
}

/// A document excerpt cited by a fact-vs-document conflict.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ConflictFileChunk {
    /// The conflicting excerpt.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub text: Option<String>,
    /// Id of the document the excerpt belongs to.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub document_id: Option<String>,
    /// Display name of that document.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub document_name: Option<String>,
    /// Fields not modeled above, preserved for output.
    #[serde(flatten)]
    pub extra: Map<String, Value>,
}

/// The stored record of how a conflict was resolved.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ConflictResolutionRecord {
    /// Resolution record id (`cflr-...`).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub id: Option<String>,
    /// Strategy applied.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub strategy: Option<String>,
    /// The fact kept, for `keep_fact`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub keep_fact_id: Option<String>,
    /// Facts forgotten by the resolution.
    #[serde(default)]
    pub forgotten_fact_ids: Vec<String>,
    /// The text each rewritten fact was resolved to.
    #[serde(default)]
    pub updated_facts: Vec<ConflictFactEdit>,
    /// When the conflict was resolved (ISO 8601).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub created_at: Option<String>,
    /// Fields not modeled above, preserved for output.
    #[serde(flatten)]
    pub extra: Map<String, Value>,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn filter_enums_use_the_wire_spelling() {
        assert_eq!(ConflictCategory::FactVsFact.as_wire(), "m2m");
        assert_eq!(ConflictCategory::FactVsDocument.as_wire(), "m2d");
        assert_eq!(ConflictCategory::SelfContradiction.as_wire(), "self");
        assert_eq!(ConflictType::Logical.as_wire(), "logical");
        assert_eq!(ConflictType::Knowledge.as_wire(), "knowledge");
    }

    #[test]
    fn a_resolved_conflict_round_trips_unchanged() {
        // Shape measured live 2026-10-09 after a `keep_fact` resolution.
        let raw = serde_json::json!({
            "id": "cfl-1870",
            "name": "User location/residency conflict",
            "description": "Fact 0 says Paris, fact 1 says Tokyo.",
            "category": "m2m",
            "resolved": true,
            "stale": false,
            "resolve": {
                "id": "cflr-3f59",
                "strategy": "keep_fact",
                "keep_fact_id": "fact-ec7a",
                "forgotten_fact_ids": ["fact-a23c"],
                "updated_facts": [],
                "created_at": "2026-10-09T07:44:55.383566Z"
            },
            "conflict_type": "logical",
            "fact_ids": ["fact-a23c", "fact-ec7a"],
            "fact_snapshots": [
                {"fact_id": "fact-a23c", "fact_text": "Paris"},
                {"fact_id": "fact-ec7a", "fact_text": "Tokyo"}
            ],
            "file_chunks": [],
            "created_at": "2026-10-09T07:43:03.83177Z",
            "updated_at": "2026-10-09T07:44:55.39195Z"
        });
        let conflict: MemoryConflict = serde_json::from_value(raw.clone()).expect("decode");
        assert_eq!(conflict.resolved, Some(true));
        assert_eq!(
            conflict
                .resolve
                .as_ref()
                .and_then(|r| r.keep_fact_id.as_deref()),
            Some("fact-ec7a")
        );
        assert_eq!(serde_json::to_value(&conflict).expect("encode"), raw);
    }

    #[test]
    fn an_unfamiliar_category_passes_through() {
        let conflict: MemoryConflict =
            serde_json::from_str(r#"{"id": "cfl-1", "category": "m2x", "future": 1}"#)
                .expect("decode");
        assert_eq!(conflict.category.as_deref(), Some("m2x"));
        assert_eq!(conflict.extra.get("future"), Some(&serde_json::json!(1)));
    }
}
