//! Get a single actor (`GET /api/v3/actors/{id}`).

use crate::api::path::encode_segment;
use crate::client::Client;
use crate::error::Result;

use super::types::Actor;

/// Path for one actor, with the id safely encoded into the segment.
pub(super) fn actor_path(id: &str) -> String {
    format!("/api/v3/actors/{}", encode_segment(id))
}

/// Fetch an actor by its server-assigned id (`act-...`).
pub fn get_actor(client: &Client, id: &str) -> Result<Actor> {
    client.get_data(&actor_path(id), &[])
}

/// Fetch an actor by the caller-defined `custom_id`.
pub fn get_actor_by_custom_id(client: &Client, custom_id: &str) -> Result<Actor> {
    client.get_data(
        &actor_path(custom_id),
        &[("by_custom_id", "true".to_string())],
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn actor_path_leaves_typical_ids_untouched() {
        assert_eq!(
            actor_path("act-a1b2c3d4e5f6"),
            "/api/v3/actors/act-a1b2c3d4e5f6"
        );
        assert_eq!(actor_path("user-ext-001"), "/api/v3/actors/user-ext-001");
    }

    #[test]
    fn actor_path_escapes_url_structural_chars() {
        assert_eq!(
            actor_path("weird id/here?foo#bar"),
            "/api/v3/actors/weird%20id%2Fhere%3Ffoo%23bar"
        );
    }

    #[test]
    fn actor_path_escapes_percent_itself() {
        // A stray `%` must be encoded so it can't be misread as a pct-triplet.
        assert_eq!(actor_path("100%off"), "/api/v3/actors/100%25off");
    }
}
