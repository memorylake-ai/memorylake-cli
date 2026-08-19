//! Create an actor (`POST /api/v3/actors`).

use serde::Serialize;
use serde_json::{Map, Value};

use crate::client::Client;
use crate::error::Result;

use super::types::{Actor, ActorType};

/// Request body for creating an actor.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct CreateActorRequest {
    /// Caller-defined external id. Must be unique account-wide.
    pub custom_id: String,
    /// Human-readable name shown in the console.
    pub display_name: String,
    /// Actor type. The server defaults to `HUMAN` when omitted.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub actor_type: Option<ActorType>,
    /// Optional free-text role or purpose.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    /// Optional labels used to group and filter actors, e.g. `vip` or `cn`.
    ///
    /// The server trims each entry, drops duplicates, and rejects a tag that is
    /// empty, longer than 64 characters, or contains a comma. At most 20 are
    /// accepted. Matching is exact and case-sensitive, so `VIP` and `vip` are
    /// two different tags.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tags: Option<Vec<String>>,
    /// Optional metadata object. Typed as a map so a non-object value cannot
    /// be sent.
    ///
    /// Use `tags` instead for values that need to be filtered on -- metadata is
    /// not filterable.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub metadata: Option<Map<String, Value>>,
}

/// Create an actor.
pub fn create_actor(client: &Client, request: &CreateActorRequest) -> Result<Actor> {
    client.post_data("/api/v3/actors", request)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn create_request_omits_unset_optional_fields() {
        let request = CreateActorRequest {
            custom_id: "user-ext-001".to_string(),
            display_name: "Alice Chen".to_string(),
            actor_type: None,
            description: None,
            tags: None,
            metadata: None,
        };
        assert_eq!(
            serde_json::to_string(&request).unwrap(),
            r#"{"custom_id":"user-ext-001","display_name":"Alice Chen"}"#
        );
    }

    #[test]
    fn create_request_serializes_actor_type_as_wire_value() {
        let request = CreateActorRequest {
            custom_id: "bot-1".to_string(),
            display_name: "Intake Bot".to_string(),
            actor_type: Some(ActorType::Assistant),
            description: Some("automated intake".to_string()),
            tags: None,
            metadata: None,
        };
        let json = serde_json::to_string(&request).unwrap();
        assert!(json.contains(r#""actor_type":"ASSISTANT""#), "{json}");
        assert!(
            json.contains(r#""description":"automated intake""#),
            "{json}"
        );
    }

    #[test]
    fn create_request_serializes_tags_as_an_array() {
        let request = CreateActorRequest {
            custom_id: "user-2".to_string(),
            display_name: "Bob".to_string(),
            actor_type: None,
            description: None,
            tags: Some(vec!["vip".to_string(), "cn".to_string()]),
            metadata: None,
        };
        let json = serde_json::to_string(&request).unwrap();
        assert!(json.contains(r#""tags":["vip","cn"]"#), "{json}");
    }

    #[test]
    fn create_request_sends_an_empty_tag_list_when_given_one() {
        // An empty list is accepted by the API and means the same as no tags.
        // It is still sent rather than dropped, because dropping it would make
        // `Some(vec![])` and `None` indistinguishable on the wire and the
        // caller asked for one of them specifically.
        let request = CreateActorRequest {
            custom_id: "user-3".to_string(),
            display_name: "Cleo".to_string(),
            actor_type: None,
            description: None,
            tags: Some(Vec::new()),
            metadata: None,
        };
        assert!(
            serde_json::to_string(&request)
                .unwrap()
                .contains(r#""tags":[]"#),
            "an explicit empty list must reach the wire"
        );
    }
}
