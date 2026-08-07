//! URL paths for the fact endpoints.
//!
//! The resolved base URL already carries the `/openapi/memorylake` service
//! prefix, so paths here start at `/api/v3`. The published docs write the full
//! prefixed path; repeating it would produce a 404.
//!
//! Facts live under their owning scope: an actor's facts sit at
//! `actors/{id}/facts`, a project's at `projects/{id}/memories/facts`. The two
//! shapes differ (only the project one carries `/memories/`), so both are
//! spelled out rather than derived.

use crate::api::path::encode_segment;

use super::types::FactScope;

/// Collection path for the facts owned by one scope.
///
/// Creating facts POSTs to this path.
pub(super) fn facts_path(workspace_id: &str, scope: &FactScope) -> String {
    match scope {
        FactScope::Actor(actor_id) => format!(
            "/api/v3/workspaces/{}/actors/{}/facts",
            encode_segment(workspace_id),
            encode_segment(actor_id)
        ),
        FactScope::Project(project_id) => format!(
            "/api/v3/workspaces/{}/projects/{}/memories/facts",
            encode_segment(workspace_id),
            encode_segment(project_id)
        ),
    }
}

/// Path that forgets one fact in one scope.
pub(super) fn forget_path(workspace_id: &str, scope: &FactScope, fact_id: &str) -> String {
    format!(
        "{}/{}/forget",
        facts_path(workspace_id, scope),
        encode_segment(fact_id)
    )
}

/// Workspace-wide fact listing path (filtered by query parameters).
pub(super) fn workspace_facts_path(workspace_id: &str) -> String {
    format!(
        "/api/v3/workspaces/{}/memories/facts",
        encode_segment(workspace_id)
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    fn actor(id: &str) -> FactScope {
        FactScope::Actor(id.to_string())
    }

    fn project(id: &str) -> FactScope {
        FactScope::Project(id.to_string())
    }

    #[test]
    fn actor_scope_omits_the_memories_segment() {
        assert_eq!(
            facts_path("ws-1", &actor("actor-8c58")),
            "/api/v3/workspaces/ws-1/actors/actor-8c58/facts"
        );
    }

    #[test]
    fn project_scope_carries_the_memories_segment() {
        assert_eq!(
            facts_path("ws-1", &project("proj-9828")),
            "/api/v3/workspaces/ws-1/projects/proj-9828/memories/facts"
        );
    }

    #[test]
    fn forget_appends_the_fact_id_and_verb() {
        assert_eq!(
            forget_path("ws-1", &actor("actor-a"), "fact-8c8a"),
            "/api/v3/workspaces/ws-1/actors/actor-a/facts/fact-8c8a/forget"
        );
        assert_eq!(
            forget_path("ws-1", &project("proj-p"), "fact-x"),
            "/api/v3/workspaces/ws-1/projects/proj-p/memories/facts/fact-x/forget"
        );
    }

    #[test]
    fn workspace_listing_path_has_no_scope_segment() {
        assert_eq!(
            workspace_facts_path("ws-63ab"),
            "/api/v3/workspaces/ws-63ab/memories/facts"
        );
    }

    #[test]
    fn every_segment_is_encoded_independently() {
        assert_eq!(
            forget_path("ws a/b", &actor("act#c"), "fact?d"),
            "/api/v3/workspaces/ws%20a%2Fb/actors/act%23c/facts/fact%3Fd/forget"
        );
    }

    #[test]
    fn a_traversal_attempt_cannot_escape_its_segment() {
        assert_eq!(
            facts_path("ws-1", &project("../..")),
            "/api/v3/workspaces/ws-1/projects/..%2F../memories/facts"
        );
    }
}
