//! Shared fact resource types.

use serde::{Deserialize, Serialize};
use serde_json::{Map, Value};

/// The single scope a fact belongs to.
///
/// Facts are strictly owned: one fact lives under exactly one actor, project,
/// or agent, and every fact operation must name that scope. The same three
/// shapes also address a scope's memory conflicts and memory settings, so this
/// enum is shared by all of them.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum FactScope {
    /// Facts attributed to an actor (`actors/{id}/facts`).
    Actor(String),
    /// Facts attached to a project (`projects/{id}/memories/facts`).
    Project(String),
    /// Facts attributed to an agent (`agents/{id}/facts`).
    Agent(String),
}

/// The owning scope the API reports on a listed fact.
///
/// Mirrors [`FactScope`] but stays stringly typed: `type` values other than
/// `actor` / `project` / `agent` added server-side must reach the caller
/// unchanged rather than fail the whole page.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct FactOwner {
    /// Scope kind: `actor`, `project`, or `agent`.
    #[serde(rename = "type")]
    pub owner_type: String,
    /// Id of the owning actor, project, or agent.
    pub id: String,
}

/// One stored fact.
///
/// The fact text lives under the wire key `fact` — the v2 API called it
/// `content`, and mixing the two up decodes every fact as empty, so the field
/// is named for the wire and documented here rather than renamed.
///
/// Absent optional fields stay absent when re-serialized, and fields this
/// struct does not model (such as a search `score`) are kept in `extra`, so
/// printing a decoded fact reproduces what the server sent.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Fact {
    /// Server-assigned fact id.
    pub id: String,
    /// The fact text. Absent in no observed response, but optional so one
    /// malformed item cannot fail a whole page.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub fact: Option<String>,
    /// Caller-defined metadata. Creation responses omit it; reads return `{}`
    /// when none was set (measured 2026-10-09).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub metadata: Option<Map<String, Value>>,
    /// Owning scope. Present on workspace listings; per-scope reads and
    /// creation responses omit it because the request path already names the
    /// scope.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub owner: Option<FactOwner>,
    /// Whether the fact has expired; absent means it has not. Creation
    /// responses omit it. A forgotten fact reads back through its trace as
    /// expired.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub expired: Option<bool>,
    /// When the fact expires or expired, if ever (ISO 8601).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub expiration_date: Option<String>,
    /// Creation timestamp (ISO 8601).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub created_at: Option<String>,
    /// Last update timestamp (ISO 8601).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub updated_at: Option<String>,
    /// Fields not modeled above, preserved for output.
    #[serde(flatten)]
    pub extra: Map<String, Value>,
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
        assert_eq!(fact.expired, None);
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
    fn metadata_expiry_and_unmodeled_fields_survive_a_round_trip() {
        // Shape measured live 2026-10-09 (the trace of a forgotten fact), plus
        // a `score` the struct does not model.
        let raw = serde_json::json!({
            "id": "fact-08aa",
            "fact": "probe",
            "metadata": {"n": [1, 2]},
            "expired": true,
            "expiration_date": "2026-10-09T07:41:35.009856Z",
            "created_at": "2026-10-09T07:40:45.972353Z",
            "updated_at": "2026-10-09T07:41:35.009856Z",
            "score": 0.5
        });
        let fact: Fact = serde_json::from_value(raw.clone()).expect("decode");
        assert_eq!(
            fact.metadata.as_ref().and_then(|map| map.get("n")),
            Some(&serde_json::json!([1, 2]))
        );
        assert_eq!(serde_json::to_value(&fact).expect("encode"), raw);
    }

    #[test]
    fn absent_fields_stay_absent_when_printed() {
        let fact: Fact = serde_json::from_str(r#"{"id": "fact-1", "fact": "t"}"#).expect("decode");
        assert_eq!(
            serde_json::to_value(&fact).expect("encode"),
            serde_json::json!({"id": "fact-1", "fact": "t"})
        );
    }

    #[test]
    fn an_agent_owner_decodes() {
        // Measured live 2026-10-09 on the workspace listing filtered by agent.
        let fact: Fact = serde_json::from_str(
            r#"{"id": "fact-1", "owner": {"type": "agent", "id": "agent-110f"}}"#,
        )
        .expect("decode");
        assert_eq!(fact.owner.expect("owner").owner_type, "agent");
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
