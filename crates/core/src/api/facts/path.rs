//! URL paths for the fact, memory-conflict, and memory-settings endpoints.
//!
//! The resolved base URL already carries the `/openapi/memorylake` service
//! prefix, so paths here start at `/api/v3`. The published docs write the full
//! prefixed path; repeating it would produce a 404.
//!
//! Everything here hangs off one scope root — `actors/{id}`, `projects/{id}`,
//! or `agents/{id}` under the workspace. The facts collection is the one
//! irregular shape: only the project one carries `/memories/`
//! (`projects/{id}/memories/facts` versus `actors/{id}/facts`), so it is
//! spelled out per scope rather than derived. Conflicts sit at
//! `{root}/memories/conflicts` and settings at `{root}/settings` for all three.

use crate::api::path::encode_segment;

use super::types::FactScope;

/// Root of one scope: `/api/v3/workspaces/{ws}/{actors|projects|agents}/{id}`.
fn scope_root(workspace_id: &str, scope: &FactScope) -> String {
    let (collection, id) = match scope {
        FactScope::Actor(id) => ("actors", id),
        FactScope::Project(id) => ("projects", id),
        FactScope::Agent(id) => ("agents", id),
    };
    format!(
        "/api/v3/workspaces/{}/{collection}/{}",
        encode_segment(workspace_id),
        encode_segment(id)
    )
}

/// Collection path for the facts owned by one scope.
///
/// Creating facts POSTs to this path.
pub(super) fn facts_path(workspace_id: &str, scope: &FactScope) -> String {
    let root = scope_root(workspace_id, scope);
    match scope {
        FactScope::Actor(_) | FactScope::Agent(_) => format!("{root}/facts"),
        FactScope::Project(_) => format!("{root}/memories/facts"),
    }
}

/// Path of one fact in one scope (read and update).
pub(super) fn fact_path(workspace_id: &str, scope: &FactScope, fact_id: &str) -> String {
    format!(
        "{}/{}",
        facts_path(workspace_id, scope),
        encode_segment(fact_id)
    )
}

/// Path that forgets one fact in one scope.
pub(super) fn forget_path(workspace_id: &str, scope: &FactScope, fact_id: &str) -> String {
    format!("{}/forget", fact_path(workspace_id, scope, fact_id))
}

/// Path of one fact's change history.
pub(super) fn trace_path(workspace_id: &str, scope: &FactScope, fact_id: &str) -> String {
    format!("{}/trace", fact_path(workspace_id, scope, fact_id))
}

/// Workspace-wide fact listing path (filtered by query parameters).
pub(super) fn workspace_facts_path(workspace_id: &str) -> String {
    format!(
        "/api/v3/workspaces/{}/memories/facts",
        encode_segment(workspace_id)
    )
}

/// Collection path for one scope's memory conflicts.
pub(super) fn conflicts_path(workspace_id: &str, scope: &FactScope) -> String {
    format!("{}/memories/conflicts", scope_root(workspace_id, scope))
}

/// Path of one memory conflict.
pub(super) fn conflict_path(workspace_id: &str, scope: &FactScope, conflict_id: &str) -> String {
    format!(
        "{}/{}",
        conflicts_path(workspace_id, scope),
        encode_segment(conflict_id)
    )
}

/// Path that resolves one memory conflict.
pub(super) fn resolve_conflict_path(
    workspace_id: &str,
    scope: &FactScope,
    conflict_id: &str,
) -> String {
    format!(
        "{}/resolve",
        conflict_path(workspace_id, scope, conflict_id)
    )
}

/// Path of one scope's memory settings.
pub(super) fn settings_path(workspace_id: &str, scope: &FactScope) -> String {
    format!("{}/settings", scope_root(workspace_id, scope))
}

/// Path that drafts (without saving) a fact instruction for one scope.
pub(super) fn draft_instruction_path(workspace_id: &str, scope: &FactScope) -> String {
    format!(
        "{}/fact-instruction/draft",
        settings_path(workspace_id, scope)
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

    fn agent(id: &str) -> FactScope {
        FactScope::Agent(id.to_string())
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
    fn agent_scope_omits_the_memories_segment() {
        assert_eq!(
            facts_path("ws-1", &agent("agent-110f")),
            "/api/v3/workspaces/ws-1/agents/agent-110f/facts"
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
        assert_eq!(
            forget_path("ws-1", &agent("agent-g"), "fact-y"),
            "/api/v3/workspaces/ws-1/agents/agent-g/facts/fact-y/forget"
        );
    }

    #[test]
    fn single_fact_and_trace_paths_follow_the_collection() {
        assert_eq!(
            fact_path("ws-1", &project("proj-p"), "fact-x"),
            "/api/v3/workspaces/ws-1/projects/proj-p/memories/facts/fact-x"
        );
        assert_eq!(
            trace_path("ws-1", &actor("actor-a"), "fact-x"),
            "/api/v3/workspaces/ws-1/actors/actor-a/facts/fact-x/trace"
        );
        assert_eq!(
            trace_path("ws-1", &agent("agent-g"), "fact-x"),
            "/api/v3/workspaces/ws-1/agents/agent-g/facts/fact-x/trace"
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
    fn conflicts_carry_the_memories_segment_in_every_scope() {
        assert_eq!(
            conflicts_path("ws-1", &actor("actor-a")),
            "/api/v3/workspaces/ws-1/actors/actor-a/memories/conflicts"
        );
        assert_eq!(
            conflict_path("ws-1", &project("proj-p"), "cfl-1"),
            "/api/v3/workspaces/ws-1/projects/proj-p/memories/conflicts/cfl-1"
        );
        assert_eq!(
            resolve_conflict_path("ws-1", &agent("agent-g"), "cfl-1"),
            "/api/v3/workspaces/ws-1/agents/agent-g/memories/conflicts/cfl-1/resolve"
        );
    }

    #[test]
    fn settings_hang_directly_off_the_scope_root() {
        assert_eq!(
            settings_path("ws-1", &project("proj-p")),
            "/api/v3/workspaces/ws-1/projects/proj-p/settings"
        );
        assert_eq!(
            settings_path("ws-1", &actor("actor-a")),
            "/api/v3/workspaces/ws-1/actors/actor-a/settings"
        );
        assert_eq!(
            draft_instruction_path("ws-1", &agent("agent-g")),
            "/api/v3/workspaces/ws-1/agents/agent-g/settings/fact-instruction/draft"
        );
    }

    #[test]
    fn every_segment_is_encoded_independently() {
        assert_eq!(
            forget_path("ws a/b", &actor("act#c"), "fact?d"),
            "/api/v3/workspaces/ws%20a%2Fb/actors/act%23c/facts/fact%3Fd/forget"
        );
        assert_eq!(
            resolve_conflict_path("ws-1", &agent("ag/x"), "cfl 1"),
            "/api/v3/workspaces/ws-1/agents/ag%2Fx/memories/conflicts/cfl%201/resolve"
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
