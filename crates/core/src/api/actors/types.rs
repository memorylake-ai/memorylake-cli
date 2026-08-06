//! Shared actor resource types.

use std::fmt;

use serde::{Deserialize, Serialize};

/// Wire value for [`ActorType::Human`].
pub const ACTOR_TYPE_HUMAN: &str = "HUMAN";

/// Wire value for [`ActorType::Assistant`].
pub const ACTOR_TYPE_ASSISTANT: &str = "ASSISTANT";

/// Whether an actor is an end user or an AI agent.
///
/// Deserialization is deliberately lenient: a value this build does not know is
/// preserved verbatim in [`ActorType::Other`] rather than failing, so a
/// server-side addition cannot break listing or fetching actors. Callers that
/// need strictness (e.g. validating a command-line flag) must check up front.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(from = "String", into = "String")]
pub enum ActorType {
    /// An end user interacting with your application.
    Human,
    /// An AI agent (created automatically alongside an Agent).
    Assistant,
    /// A type this build does not recognize, kept as returned by the server.
    Other(String),
}

impl ActorType {
    /// Wire representation of this actor type.
    pub fn as_str(&self) -> &str {
        match self {
            Self::Human => ACTOR_TYPE_HUMAN,
            Self::Assistant => ACTOR_TYPE_ASSISTANT,
            Self::Other(raw) => raw,
        }
    }
}

impl fmt::Display for ActorType {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

impl From<String> for ActorType {
    fn from(raw: String) -> Self {
        match raw.as_str() {
            ACTOR_TYPE_HUMAN => Self::Human,
            ACTOR_TYPE_ASSISTANT => Self::Assistant,
            _ => Self::Other(raw),
        }
    }
}

impl From<ActorType> for String {
    fn from(value: ActorType) -> Self {
        match value {
            ActorType::Human => ACTOR_TYPE_HUMAN.to_string(),
            ActorType::Assistant => ACTOR_TYPE_ASSISTANT.to_string(),
            ActorType::Other(raw) => raw,
        }
    }
}

/// A MemoryLake actor: the participant identity every memory is attributed to.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Actor {
    /// Server-assigned actor id (`act-...`).
    pub id: String,
    /// Caller-defined external id, unique account-wide.
    #[serde(default)]
    pub custom_id: Option<String>,
    /// Whether this actor is a human or an assistant.
    pub actor_type: ActorType,
    /// Human-readable name shown in the console.
    pub display_name: String,
    /// Optional free-text role or purpose.
    #[serde(default)]
    pub description: Option<String>,
    /// Optional caller-defined metadata.
    #[serde(default)]
    pub metadata: Option<serde_json::Value>,
    /// Creation timestamp (ISO 8601).
    #[serde(default)]
    pub created_at: Option<String>,
    /// Last update timestamp (ISO 8601).
    #[serde(default)]
    pub updated_at: Option<String>,
}

/// An actor's membership in one workspace.
///
/// Returned by the bind and workspace-listing endpoints. This is a different
/// shape from [`Actor`]: it identifies the actor with `actor_id` and carries
/// the binding timestamp instead of the full actor record.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ActorBinding {
    /// Id of the bound actor.
    pub actor_id: String,
    /// Bound actor's caller-defined external id.
    #[serde(default)]
    pub custom_id: Option<String>,
    /// Bound actor's type.
    #[serde(default)]
    pub actor_type: Option<ActorType>,
    /// Bound actor's display name.
    #[serde(default)]
    pub display_name: Option<String>,
    /// When the binding was created (ISO 8601).
    #[serde(default)]
    pub bound_at: Option<String>,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn actor_type_round_trips_known_values() {
        let human: ActorType = serde_json::from_str("\"HUMAN\"").expect("deserialize HUMAN");
        assert_eq!(human, ActorType::Human);
        assert_eq!(serde_json::to_string(&human).unwrap(), "\"HUMAN\"");

        let assistant: ActorType =
            serde_json::from_str("\"ASSISTANT\"").expect("deserialize ASSISTANT");
        assert_eq!(assistant, ActorType::Assistant);
        assert_eq!(serde_json::to_string(&assistant).unwrap(), "\"ASSISTANT\"");
    }

    #[test]
    fn actor_type_preserves_unknown_values() {
        // A type added server-side must not fail decoding, and must survive a
        // round trip so the raw value stays visible to the user.
        let unknown: ActorType = serde_json::from_str("\"WEBHOOK\"").expect("deserialize unknown");
        assert_eq!(unknown, ActorType::Other("WEBHOOK".to_string()));
        assert_eq!(unknown.as_str(), "WEBHOOK");
        assert_eq!(serde_json::to_string(&unknown).unwrap(), "\"WEBHOOK\"");
    }

    #[test]
    fn actor_decodes_unrecognized_actor_type() {
        let raw = r#"{
            "id": "act-a1b2c3d4e5f6",
            "custom_id": "user-ext-001",
            "actor_type": "SUPERVISOR",
            "display_name": "Alice Chen"
        }"#;
        let actor: Actor = serde_json::from_str(raw).expect("decode actor with unknown type");
        assert_eq!(actor.actor_type, ActorType::Other("SUPERVISOR".to_string()));
        assert_eq!(actor.display_name, "Alice Chen");
        assert!(actor.description.is_none());
        assert!(actor.metadata.is_none());
    }

    #[test]
    fn actor_binding_decodes_documented_shape() {
        let raw = r#"{
            "actor_id": "act-a1b2c3d4e5f6",
            "custom_id": "user-ext-001",
            "actor_type": "HUMAN",
            "display_name": "Alice Chen",
            "bound_at": "2025-03-15T09:00:00Z"
        }"#;
        let binding: ActorBinding = serde_json::from_str(raw).expect("decode binding");
        assert_eq!(binding.actor_id, "act-a1b2c3d4e5f6");
        assert_eq!(binding.actor_type, Some(ActorType::Human));
        assert_eq!(binding.bound_at.as_deref(), Some("2025-03-15T09:00:00Z"));
    }
}
