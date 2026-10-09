//! Read one fact's change history (`GET .../facts/{fact_id}/trace`).

use serde::{Deserialize, Serialize};
use serde_json::{Map, Value};

use crate::client::Client;
use crate::error::Result;

use super::path::trace_path;
use super::types::{Fact, FactScope};

/// A fact together with its full change history.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct FactTrace {
    /// The fact as it stands now — expired, if it has been forgotten.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub fact: Option<Fact>,
    /// Change history, newest first.
    #[serde(default)]
    pub trace: Vec<FactTraceEntry>,
    /// Fields not modeled above, preserved for output.
    #[serde(flatten)]
    pub extra: Map<String, Value>,
}

/// One recorded change to a fact.
///
/// Enumerated values stay strings so a kind added server-side reaches the
/// caller instead of failing the whole history.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct FactTraceEntry {
    /// Kind of change: `ADD`, `UPDATE`, or `FORGET`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub event: Option<String>,
    /// When the change happened (ISO 8601).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub timestamp: Option<String>,
    /// Id of this history entry.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub history_id: Option<String>,
    /// How the change was produced: `COOK` (extracted from conversation
    /// content) or `MANUAL` (a direct edit or forget through the API).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub source_kind: Option<String>,
    /// Text before the change; absent for `ADD`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub old_fact: Option<String>,
    /// Text after the change; absent for `FORGET`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub new_fact: Option<String>,
    /// Conversation the change was extracted from; absent for a manual change.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub conversation_id: Option<String>,
    /// Ids of the conversation messages the change was extracted from; empty
    /// for a manual change.
    #[serde(default)]
    pub source_entry_ids: Vec<String>,
    /// Opaque id of the event that produced the change.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub source_event_id: Option<String>,
    /// Fields not modeled above, preserved for output.
    #[serde(flatten)]
    pub extra: Map<String, Value>,
}

/// Fetch a fact and its change history in one call.
///
/// Unlike [`get_fact`](super::get_fact), this still answers for a forgotten
/// fact: the history ends with its `FORGET` event.
pub fn trace_fact(
    client: &Client,
    workspace_id: &str,
    scope: &FactScope,
    fact_id: &str,
) -> Result<FactTrace> {
    client.get_data(&trace_path(workspace_id, scope, fact_id), &[])
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_measured_history_round_trips_unchanged() {
        // Shape measured live 2026-10-09: a manual edit on top of an add.
        let raw = serde_json::json!({
            "fact": {
                "id": "fact-5782",
                "fact": "edited",
                "metadata": {},
                "expired": false,
                "created_at": "2026-10-09T07:40:45.941563Z",
                "updated_at": "2026-10-09T07:41:00.258388Z"
            },
            "trace": [
                {
                    "event": "UPDATE",
                    "timestamp": "2026-10-09T07:40:58.001909Z",
                    "history_id": "facthist-2865",
                    "source_kind": "MANUAL",
                    "old_fact": "original",
                    "new_fact": "edited",
                    "source_entry_ids": [],
                    "source_event_id": "api-30d6"
                },
                {
                    "event": "ADD",
                    "timestamp": "2026-10-09T07:40:45.949196Z",
                    "history_id": "facthist-c4b7",
                    "source_kind": "MANUAL",
                    "new_fact": "original",
                    "source_entry_ids": [],
                    "source_event_id": "api-fd05"
                }
            ]
        });
        let trace: FactTrace = serde_json::from_value(raw.clone()).expect("decode");
        assert_eq!(trace.trace.len(), 2);
        assert_eq!(trace.trace[0].event.as_deref(), Some("UPDATE"));
        assert_eq!(serde_json::to_value(&trace).expect("encode"), raw);
    }

    #[test]
    fn an_empty_payload_decodes_as_no_history() {
        let trace: FactTrace = serde_json::from_str("{}").expect("decode");
        assert!(trace.fact.is_none());
        assert!(trace.trace.is_empty());
    }
}
