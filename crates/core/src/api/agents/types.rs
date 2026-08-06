//! Shared agent resource types.
//!
//! Response structs capture the documented fields and collect anything else the
//! server sends into `extra` via `#[serde(flatten)]`, so re-serializing a
//! response never silently drops data the CLI did not know about.

use serde::{Deserialize, Serialize};
use serde_json::{Map, Value};

/// Request body for agent create, update, and version-create calls.
///
/// Modeled as an open JSON object rather than a closed struct: the documented
/// fields are known, but unrecognized top-level keys are forwarded to the
/// server verbatim so the CLI keeps working when the API gains fields first.
pub type AgentRequestBody = Map<String, Value>;

/// Body keys that describe agent *configuration*.
///
/// `PATCH /api/v3/agents/{id}` accepts identity fields only; changing any of
/// these requires creating a new agent version instead.
pub const CONFIG_FIELDS: &[&str] = &[
    "model",
    "capabilities",
    "policies",
    "output",
    "subagents",
    "skills",
    "system_prompt",
    "model_settings",
    "runtime_bindings",
];

/// Body keys accepted by `PATCH /api/v3/agents/{id}`.
pub const IDENTITY_FIELDS: &[&str] = &["name", "description", "metadata"];

/// A MemoryLake agent.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Agent {
    /// Server-assigned agent id.
    pub id: String,
    /// Display name.
    pub name: String,
    /// Optional description.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    /// Caller-defined metadata object.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub metadata: Option<Value>,
    /// Version this representation reflects.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub version: Option<u64>,
    /// Highest version number that exists for this agent.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub latest_version: Option<u64>,
    /// Caller-defined stable external id.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub custom_id: Option<String>,
    /// Actor identity generated for this agent (actors are the subject of memory).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub actor_id: Option<String>,
    /// Model identifier.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub model: Option<String>,
    /// Agent instruction prompt.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub system_prompt: Option<String>,
    /// Supported capability identifiers.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub capabilities: Option<Value>,
    /// Execution constraints (`max_turns`, `allow_tools`, …).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub policies: Option<Value>,
    /// Output configuration (`mode`, `json_schema`).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub output: Option<Value>,
    /// Subagent definitions.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub subagents: Option<Value>,
    /// Attached skills.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub skills: Option<Value>,
    /// Model tuning settings (temperature, max tokens, …).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub model_settings: Option<Value>,
    /// Tool and data-source configuration.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub runtime_bindings: Option<Value>,
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

/// A single immutable configuration version of an agent.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AgentVersion {
    /// Version number.
    pub version: u64,
    /// Agent this version belongs to.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub agent_id: Option<String>,
    /// Model identifier.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub model: Option<String>,
    /// Agent instruction prompt.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub system_prompt: Option<String>,
    /// Supported capability identifiers.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub capabilities: Option<Value>,
    /// Execution constraints.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub policies: Option<Value>,
    /// Output configuration.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub output: Option<Value>,
    /// Subagent definitions.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub subagents: Option<Value>,
    /// Attached skills.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub skills: Option<Value>,
    /// Model tuning settings.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub model_settings: Option<Value>,
    /// Tool and data-source configuration.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub runtime_bindings: Option<Value>,
    /// Creation timestamp (ISO 8601).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub created_at: Option<String>,
    /// Fields returned by the server that this client does not model.
    #[serde(flatten)]
    pub extra: Map<String, Value>,
}

/// Summary of an agent bound to a workspace.
///
/// Deliberately narrower than [`Agent`]: the binding endpoints return only the
/// fields needed to identify the agent plus when the binding was made.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct WorkspaceAgentBinding {
    /// Bound agent id.
    pub agent_id: String,
    /// Agent display name.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,
    /// Caller-defined stable external id.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub custom_id: Option<String>,
    /// Highest version number that exists for the agent.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub latest_version: Option<u64>,
    /// Binding timestamp (ISO 8601).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub bound_at: Option<String>,
    /// Fields returned by the server that this client does not model.
    #[serde(flatten)]
    pub extra: Map<String, Value>,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn agent_round_trip_preserves_unmodeled_fields() {
        let raw = serde_json::json!({
            "id": "agt-1",
            "name": "Support",
            "model": "claude-sonnet-4-20250514",
            "future_field": {"nested": true}
        });

        let agent: Agent = serde_json::from_value(raw.clone()).expect("deserialize agent");
        assert_eq!(agent.id, "agt-1");
        assert_eq!(agent.model.as_deref(), Some("claude-sonnet-4-20250514"));
        assert!(agent.extra.contains_key("future_field"));

        let back = serde_json::to_value(&agent).expect("serialize agent");
        assert_eq!(back, raw, "re-serializing must not drop unmodeled fields");
    }

    #[test]
    fn agent_omits_absent_optional_fields() {
        let agent: Agent =
            serde_json::from_value(serde_json::json!({"id": "agt-1", "name": "A"})).unwrap();
        let back = serde_json::to_value(&agent).unwrap();
        assert_eq!(back, serde_json::json!({"id": "agt-1", "name": "A"}));
    }

    #[test]
    fn version_round_trip_preserves_unmodeled_fields() {
        let raw = serde_json::json!({
            "version": 3,
            "agent_id": "agt-1",
            "unknown_knob": "keep me"
        });
        let version: AgentVersion = serde_json::from_value(raw.clone()).unwrap();
        assert_eq!(version.version, 3);
        assert_eq!(serde_json::to_value(&version).unwrap(), raw);
    }

    #[test]
    fn binding_round_trip_preserves_unmodeled_fields() {
        let raw = serde_json::json!({
            "agent_id": "agt-1",
            "name": "Support",
            "bound_at": "2026-01-01T00:00:00Z",
            "extra_flag": false
        });
        let binding: WorkspaceAgentBinding = serde_json::from_value(raw.clone()).unwrap();
        assert_eq!(binding.agent_id, "agt-1");
        assert_eq!(serde_json::to_value(&binding).unwrap(), raw);
    }

    #[test]
    fn config_and_identity_fields_are_disjoint() {
        for field in CONFIG_FIELDS {
            assert!(
                !IDENTITY_FIELDS.contains(field),
                "`{field}` cannot be both identity and configuration"
            );
        }
    }
}
