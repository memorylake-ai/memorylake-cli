//! Shared fact resource types.

use serde::{Deserialize, Serialize};

/// The single scope a fact belongs to.
///
/// Facts are strictly owned: one fact lives under exactly one actor or one
/// project, and every fact operation must name that scope. Reading and writing
/// use the same two shapes, so this enum is shared by all of them.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum FactScope {
    /// Facts attributed to an actor (`actors/{id}/facts`).
    Actor(String),
    /// Facts attached to a project (`projects/{id}/memories/facts`).
    Project(String),
}

/// The owning scope the API reports on a listed fact.
///
/// Mirrors [`FactScope`] but stays stringly typed: `type` values other than
/// `actor` / `project` added server-side must reach the caller unchanged
/// rather than fail the whole page.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct FactOwner {
    /// Scope kind: `actor` or `project` in every observed response.
    #[serde(rename = "type")]
    pub owner_type: String,
    /// Id of the owning actor or project.
    pub id: String,
}

/// One stored fact.
///
/// The fact text lives under the wire key `fact` — the v2 API called it
/// `content`, and mixing the two up decodes every fact as empty, so the field
/// is named for the wire and documented here rather than renamed.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Fact {
    /// Server-assigned fact id.
    pub id: String,
    /// The fact text. Absent in no observed response, but optional so one
    /// malformed item cannot fail a whole page.
    #[serde(default)]
    pub fact: Option<String>,
    /// Owning scope. Present on workspace listings; creation responses omit it
    /// because the scope is already named in the request path.
    #[serde(default)]
    pub owner: Option<FactOwner>,
    /// Whether the fact has expired. Absent means false.
    #[serde(default)]
    pub expired: bool,
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
    fn a_listed_fact_decodes_with_owner_and_text_under_the_fact_key() {
        // Shape measured live 2026-08-07 against the workspace facts listing.
        let fact: Fact = serde_json::from_str(
            r#"{
                "id": "fact-8c8a6242be3948afa8e9a970ebdb7d47",
                "fact": "user's editor is vim",
                "owner": {"type": "actor", "id": "actor-8c588aff93d346479b8ad4e56ec3f860"}
            }"#,
        )
        .expect("decode listed fact");

        assert_eq!(fact.fact.as_deref(), Some("user's editor is vim"));
        let owner = fact.owner.expect("owner present");
        assert_eq!(owner.owner_type, "actor");
        assert!(!fact.expired);
    }

    #[test]
    fn a_creation_response_fact_decodes_without_owner() {
        let fact: Fact =
            serde_json::from_str(r#"{"id": "fact-1", "fact": "text"}"#).expect("decode");
        assert!(fact.owner.is_none());
    }

    #[test]
    fn a_content_key_is_not_mistaken_for_the_fact_text() {
        // v2 named the text `content`; decoding it as text here would silently
        // read v2 payloads as valid. It must come back as absent instead.
        let fact: Fact =
            serde_json::from_str(r#"{"id": "fact-1", "content": "v2 text"}"#).expect("decode");
        assert_eq!(fact.fact, None);
    }

    #[test]
    fn unknown_fields_do_not_break_decoding() {
        let fact: Fact =
            serde_json::from_str(r#"{"id": "fact-1", "fact": "t", "future_field": {"a": 1}}"#)
                .expect("decode");
        assert_eq!(fact.fact.as_deref(), Some("t"));
    }

    #[test]
    fn an_unfamiliar_owner_type_passes_through() {
        let fact: Fact = serde_json::from_str(
            r#"{"id": "fact-1", "owner": {"type": "workspace", "id": "ws-1"}}"#,
        )
        .expect("decode");
        assert_eq!(fact.owner.expect("owner").owner_type, "workspace");
    }
}
