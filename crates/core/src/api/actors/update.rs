//! Update an actor (`PATCH /api/v3/actors/{id}`).

use serde::Serialize;
use serde_json::{Map, Value};

use crate::client::Client;
use crate::error::Result;

use super::get::actor_path;
use super::types::Actor;

/// Request body for a partial actor update.
///
/// Only the fields present here are sent, and the server changes only what it
/// receives. `metadata` **replaces** the stored object wholesale — the server
/// does not merge — so a caller that wants to keep existing keys must send
/// them again.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize)]
pub struct UpdateActorRequest {
    /// New display name.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub display_name: Option<String>,
    /// New description.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    /// Replacement tag list.
    ///
    /// Like `metadata`, this **replaces** rather than merges: the list sent here
    /// becomes the actor's complete set of tags. `Some(vec![])` removes every
    /// tag; `None` leaves the stored tags untouched.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tags: Option<Vec<String>>,
    /// Replacement metadata object.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub metadata: Option<Map<String, Value>>,
}

impl UpdateActorRequest {
    /// `true` when no field is set, i.e. the request would send an empty body
    /// and change nothing. Callers should reject this before making a request.
    pub fn is_empty(&self) -> bool {
        self.display_name.is_none()
            && self.description.is_none()
            && self.tags.is_none()
            && self.metadata.is_none()
    }
}

/// Apply a partial update to an actor.
pub fn update_actor(client: &Client, id: &str, request: &UpdateActorRequest) -> Result<Actor> {
    client.patch_data(&actor_path(id), request)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_request_is_empty() {
        assert!(UpdateActorRequest::default().is_empty());
    }

    #[test]
    fn request_with_any_field_is_not_empty() {
        let request = UpdateActorRequest {
            description: Some("updated".to_string()),
            ..UpdateActorRequest::default()
        };
        assert!(!request.is_empty());
        assert_eq!(
            serde_json::to_string(&request).unwrap(),
            r#"{"description":"updated"}"#
        );
    }

    #[test]
    fn metadata_serializes_as_a_bare_object() {
        let mut metadata = Map::new();
        metadata.insert("tier".to_string(), Value::String("enterprise".to_string()));
        let request = UpdateActorRequest {
            metadata: Some(metadata),
            ..UpdateActorRequest::default()
        };
        assert_eq!(
            serde_json::to_string(&request).unwrap(),
            r#"{"metadata":{"tier":"enterprise"}}"#
        );
    }

    #[test]
    fn tags_alone_is_a_valid_update() {
        let request = UpdateActorRequest {
            tags: Some(vec!["gold".to_string()]),
            ..UpdateActorRequest::default()
        };
        assert!(!request.is_empty());
        assert_eq!(
            serde_json::to_string(&request).unwrap(),
            r#"{"tags":["gold"]}"#
        );
    }

    #[test]
    fn an_empty_tag_list_is_sent_because_it_means_remove_every_tag() {
        // The distinction matters here in a way it does not for the other
        // fields: an omitted `tags` leaves the stored tags alone, so dropping
        // an empty list would silently turn "clear them" into "change nothing".
        let request = UpdateActorRequest {
            tags: Some(Vec::new()),
            ..UpdateActorRequest::default()
        };
        assert!(!request.is_empty());
        assert_eq!(serde_json::to_string(&request).unwrap(), r#"{"tags":[]}"#);
    }
}
