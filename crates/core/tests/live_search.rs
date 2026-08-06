//! Live integration tests for the memory search API.
//!
//! Requires `MEMORYLAKE_API_KEY` (from the environment or repo-root `.env`).
//! Missing or empty key fails the tests (no skip).
//!
//! This crate cannot ingest memories, so a freshly created workspace has
//! nothing to match. These tests prove the request is accepted and the response
//! decodes into the documented shape; they cannot prove retrieval is relevant.

mod common;

use std::time::{SystemTime, UNIX_EPOCH};

use memorylake_core::Client;
use memorylake_core::api::search::{MemoryType, SearchMemoriesRequest, search_memories};
use memorylake_core::api::workspaces::{CreateWorkspaceRequest, create_workspace};

use common::live_client;

fn nanos() -> u128 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("clock")
        .as_nanos()
}

/// Create a workspace to search in and return its id.
///
/// The workspace is left behind: `delete_workspace` is not implemented.
fn host_workspace(client: &Client, label: &str) -> String {
    let stamp = nanos();
    create_workspace(
        client,
        &CreateWorkspaceRequest {
            name: format!("Core Live search {label} {stamp}"),
            custom_id: format!("core-live-search-{label}-{stamp}"),
            description: Some("created by memorylake-core search live test".into()),
        },
    )
    .expect("create host workspace")
    .id
}

#[test]
fn query_only_search_is_accepted_and_decodes() {
    let client = live_client();
    let workspace_id = host_workspace(&client, "query");

    let results = search_memories(
        &client,
        &workspace_id,
        &SearchMemoriesRequest::new("what were the quarterly revenue figures"),
    )
    .expect("search with only a query");

    // An empty workspace matches nothing; the point is that it decoded.
    assert!(results.documents.is_empty());
    assert!(results.facts.is_empty());
}

#[test]
fn memory_type_and_top_k_filters_are_accepted() {
    let client = live_client();
    let workspace_id = host_workspace(&client, "filters");

    for types in [
        vec![MemoryType::Document, MemoryType::Fact],
        vec![MemoryType::Document],
        vec![MemoryType::Fact],
    ] {
        search_memories(
            &client,
            &workspace_id,
            &SearchMemoriesRequest {
                memory_types: Some(types.clone()),
                top_k: Some(5),
                ..SearchMemoriesRequest::new("revenue")
            },
        )
        .unwrap_or_else(|err| panic!("search with memory_types {types:?} failed: {err}"));
    }
}

#[test]
fn scope_filters_are_validated_against_the_workspace() {
    // Undocumented but consistent: the server does not silently ignore a scope
    // it cannot resolve, it rejects the request. A caller passing a project or
    // actor from the wrong workspace gets an error, not empty results.
    let client = live_client();
    let workspace_id = host_workspace(&client, "scope");

    let unknown_project = search_memories(
        &client,
        &workspace_id,
        &SearchMemoriesRequest {
            project_ids: Some(vec![format!("proj-none-{}", nanos())]),
            ..SearchMemoriesRequest::new("revenue")
        },
    );
    assert!(
        unknown_project.is_err(),
        "a project outside the workspace should be rejected, got {unknown_project:?}"
    );

    let unbound_actor = search_memories(
        &client,
        &workspace_id,
        &SearchMemoriesRequest {
            actor_ids: Some(vec![format!("act-none-{}", nanos())]),
            ..SearchMemoriesRequest::new("revenue")
        },
    );
    assert!(
        unbound_actor.is_err(),
        "an actor not bound to the workspace should be rejected, got {unbound_actor:?}"
    );
}

#[test]
fn top_k_is_bounded_by_the_server() {
    // The v3 docs state no range; the server enforces 1..=1000. The CLI
    // deliberately does not duplicate that bound, so this records where it lives.
    let client = live_client();
    let workspace_id = host_workspace(&client, "topk");

    for out_of_range in [0, 100_000] {
        let result = search_memories(
            &client,
            &workspace_id,
            &SearchMemoriesRequest {
                top_k: Some(out_of_range),
                ..SearchMemoriesRequest::new("revenue")
            },
        );
        assert!(
            result.is_err(),
            "top_k {out_of_range} should be rejected, got {result:?}"
        );
    }

    search_memories(
        &client,
        &workspace_id,
        &SearchMemoriesRequest {
            top_k: Some(1),
            ..SearchMemoriesRequest::new("revenue")
        },
    )
    .expect("top_k at the lower bound should be accepted");
}

#[test]
fn search_in_an_unknown_workspace_is_an_error() {
    let client = live_client();

    let missing = search_memories(
        &client,
        "ws-does-not-exist-000000000000",
        &SearchMemoriesRequest::new("anything"),
    );
    assert!(
        missing.is_err(),
        "expected an error for an unknown workspace, got {missing:?}"
    );
}
