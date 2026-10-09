//! Copy an agent (`POST /api/v3/agents/{id}/fork`).

use std::collections::BTreeMap;

use serde::Serialize;

use crate::client::Client;
use crate::error::Result;

use super::agent_path;
use super::types::Agent;

/// Longest name the API accepts for the copy, in characters.
pub const FORK_NAME_MAX_CHARS: usize = 255;

/// Request body for copying an agent.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct ForkAgentRequest {
    /// Caller-defined unique id for the copy. Required, which is what makes a
    /// retried fork conflict (`409 CUSTOM_ID_CONFLICT`) instead of producing a
    /// second copy.
    pub custom_id: String,
    /// Display name of the copy, 1–[`FORK_NAME_MAX_CHARS`] characters. The
    /// server defaults to the source's name plus ` (copy)`.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,
    /// Metadata for the copy. Never inherited from the source agent.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub metadata: Option<BTreeMap<String, String>>,
}

/// `/api/v3/agents/{id}/fork`
fn agent_fork_path(id: &str) -> String {
    format!("{}/fork", agent_path(id))
}

/// Copy an agent into a new, fully independent one with its own id and actor.
///
/// The copy carries the source's current configuration; editing either agent
/// afterwards does not affect the other. Workspace bindings are not copied.
/// An `EXTERNAL` agent cannot be copied and is refused with
/// `AGENT_OPERATION_NOT_SUPPORTED`.
pub fn fork_agent(client: &Client, id: &str, request: &ForkAgentRequest) -> Result<Agent> {
    client.post_data(&agent_fork_path(id), request)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn fork_path_appends_the_verb() {
        assert_eq!(agent_fork_path("agent-1"), "/api/v3/agents/agent-1/fork");
        assert_eq!(agent_fork_path("a/../b"), "/api/v3/agents/a%2F..%2Fb/fork");
    }

    #[test]
    fn only_custom_id_is_sent_when_nothing_else_is_set() {
        let body = serde_json::to_string(&ForkAgentRequest {
            custom_id: "copy-1".into(),
            name: None,
            metadata: None,
        })
        .unwrap();
        assert_eq!(body, r#"{"custom_id":"copy-1"}"#);
    }

    #[test]
    fn name_and_metadata_are_sent_when_set() {
        let body = serde_json::to_string(&ForkAgentRequest {
            custom_id: "copy-1".into(),
            name: Some("Copy".into()),
            metadata: Some(BTreeMap::from([("k".to_string(), "v".to_string())])),
        })
        .unwrap();
        assert_eq!(
            body,
            r#"{"custom_id":"copy-1","name":"Copy","metadata":{"k":"v"}}"#
        );
    }
}
