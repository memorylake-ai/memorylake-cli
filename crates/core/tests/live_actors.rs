//! Live actor integration tests against a real MemoryLake environment.
//!
//! Requires `MEMORYLAKE_API_KEY` (from the environment or repo-root `.env`).
//! Missing or empty key fails the tests (no skip).

mod common;

use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{SystemTime, UNIX_EPOCH};

use memorylake_core::Client;
use memorylake_core::api::actors::{
    Actor, ActorType, CreateActorRequest, ListActorsParams, UpdateActorRequest, bind_actor,
    create_actor, delete_actor, get_actor, get_actor_by_custom_id, list_actors,
    list_workspace_actors, unbind_actor, update_actor,
};
use memorylake_core::api::workspaces::{CreateWorkspaceRequest, create_workspace};
use serde_json::{Map, Value};

use common::live_client;

/// Deletes the actor it guards on drop, so a failed assertion cannot leave data
/// behind in a real account. Disarm it after an asserted delete.
struct ActorGuard {
    client: Client,
    id: String,
}

impl ActorGuard {
    fn new(client: &Client, id: &str) -> Self {
        Self {
            client: client.clone(),
            id: id.to_string(),
        }
    }

    /// Stop guarding, once the test has deleted the actor itself.
    fn disarm(&mut self) {
        self.id.clear();
    }
}

impl Drop for ActorGuard {
    fn drop(&mut self) {
        if self.id.is_empty() {
            return;
        }
        // Best effort: the test has already reported the real failure, and a
        // panic here would mask it.
        let _ = delete_actor(&self.client, &self.id);
    }
}

/// A suffix unique across parallel tests and repeated runs.
///
/// `custom_id` is unique account-wide, and the system clock is not fine-grained
/// enough to separate tests that start in the same instant, so mix in the
/// process id and a counter.
fn unique_suffix() -> String {
    static NEXT: AtomicU64 = AtomicU64::new(0);
    format!(
        "{}-{}-{}",
        std::process::id(),
        NEXT.fetch_add(1, Ordering::Relaxed),
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("clock")
            .as_nanos()
    )
}

fn metadata(pairs: &[(&str, &str)]) -> Map<String, Value> {
    pairs
        .iter()
        .map(|(key, value)| ((*key).to_string(), Value::String((*value).to_string())))
        .collect()
}

fn create_live_actor(client: &Client, suffix: &str) -> Actor {
    create_actor(
        client,
        &CreateActorRequest {
            custom_id: format!("cli-live-actor-{suffix}"),
            display_name: format!("CLI Live Actor {suffix}"),
            actor_type: Some(ActorType::Human),
            description: Some("created by memorylake-cli live test".into()),
            tags: Some(vec!["cli-live".into(), "vip".into()]),
            metadata: Some(metadata(&[("tier", "premium"), ("region", "us-west")])),
        },
    )
    .expect("create actor")
}

#[test]
fn create_get_update_and_delete_actor() {
    let client = live_client();
    let suffix = unique_suffix();

    let created = create_live_actor(&client, &suffix);
    let mut guard = ActorGuard::new(&client, &created.id);

    assert!(!created.id.is_empty());
    assert_eq!(
        created.custom_id.as_deref(),
        Some(format!("cli-live-actor-{suffix}").as_str())
    );
    assert_eq!(created.actor_type, ActorType::Human);
    assert_eq!(created.display_name, format!("CLI Live Actor {suffix}"));
    assert_eq!(
        created.tags,
        vec!["cli-live".to_string(), "vip".to_string()],
        "tags sent on create must come back on the created actor"
    );

    let by_id = get_actor(&client, &created.id).expect("get by id");
    assert_eq!(by_id.id, created.id);

    let by_custom = get_actor_by_custom_id(&client, &format!("cli-live-actor-{suffix}"))
        .expect("get by custom_id");
    assert_eq!(by_custom.id, created.id);

    // `metadata` replaces the whole object, and omitted fields are untouched.
    let updated = update_actor(
        &client,
        &created.id,
        &UpdateActorRequest {
            display_name: Some(format!("CLI Live Actor {suffix} (VIP)")),
            metadata: Some(metadata(&[("tier", "enterprise")])),
            ..UpdateActorRequest::default()
        },
    )
    .expect("update actor");
    assert_eq!(
        updated.display_name,
        format!("CLI Live Actor {suffix} (VIP)")
    );
    assert_eq!(
        updated.description.as_deref(),
        Some("created by memorylake-cli live test"),
        "an omitted field must be left untouched"
    );
    let updated_metadata = updated.metadata.clone().expect("metadata present");
    assert_eq!(
        updated_metadata.get("tier"),
        Some(&Value::String("enterprise".to_string()))
    );
    assert!(
        updated_metadata.get("region").is_none(),
        "metadata must be replaced wholesale, not merged: {updated_metadata}"
    );
    assert_eq!(
        updated.tags,
        vec!["cli-live".to_string(), "vip".to_string()],
        "an update that does not mention tags must leave them alone"
    );

    // Tags replace wholesale too, and an empty list is how they are removed.
    let retagged = update_actor(
        &client,
        &created.id,
        &UpdateActorRequest {
            tags: Some(vec!["gold".into()]),
            ..UpdateActorRequest::default()
        },
    )
    .expect("replace tags");
    assert_eq!(retagged.tags, vec!["gold".to_string()]);

    let untagged = update_actor(
        &client,
        &created.id,
        &UpdateActorRequest {
            tags: Some(Vec::new()),
            ..UpdateActorRequest::default()
        },
    )
    .expect("clear tags");
    assert!(
        untagged.tags.is_empty(),
        "an empty list must remove every tag, not be ignored: {:?}",
        untagged.tags
    );

    delete_actor(&client, &created.id).expect("delete actor");
    guard.disarm();

    let missing = get_actor(&client, &created.id);
    assert!(
        missing.is_err(),
        "expected an error for a deleted actor, got {missing:?}"
    );
}

#[test]
fn list_actors_finds_a_created_actor() {
    let client = live_client();
    let suffix = unique_suffix();

    let created = create_live_actor(&client, &suffix);
    let mut guard = ActorGuard::new(&client, &created.id);

    let page = list_actors(
        &client,
        &ListActorsParams {
            page_size: Some(50),
            actor_type: Some(ActorType::Human),
            display_name_fuzzy: Some(created.display_name.clone()),
            ..ListActorsParams::default()
        },
    )
    .expect("list actors");

    assert!(
        page.items.iter().any(|actor| actor.id == created.id),
        "created actor {} not found in list results",
        created.id
    );

    delete_actor(&client, &created.id).expect("delete actor");
    guard.disarm();
}

#[test]
fn bind_list_and_unbind_workspace_actors() {
    let client = live_client();
    let suffix = unique_suffix();

    let created = create_live_actor(&client, &suffix);
    let mut guard = ActorGuard::new(&client, &created.id);

    // No delete-workspace endpoint is exposed, so — like the existing workspace
    // live tests — this workspace is left behind.
    let workspace = create_workspace(
        &client,
        &CreateWorkspaceRequest {
            name: format!("CLI Live Actor WS {suffix}"),
            custom_id: format!("cli-live-actor-ws-{suffix}"),
            description: None,
        },
    )
    .expect("create workspace");

    let binding = bind_actor(&client, &workspace.id, &created.id).expect("bind actor");
    assert_eq!(binding.actor_id, created.id);

    let bound = list_workspace_actors(
        &client,
        &workspace.id,
        &ListActorsParams {
            page_size: Some(50),
            ..ListActorsParams::default()
        },
    )
    .expect("list workspace actors");
    assert!(
        bound.items.iter().any(|item| item.actor_id == created.id),
        "bound actor {} not found in workspace {}",
        created.id,
        workspace.id
    );

    unbind_actor(&client, &workspace.id, &created.id).expect("unbind actor");

    let after = list_workspace_actors(
        &client,
        &workspace.id,
        &ListActorsParams {
            page_size: Some(50),
            ..ListActorsParams::default()
        },
    )
    .expect("list workspace actors after unbind");
    assert!(
        !after.items.iter().any(|item| item.actor_id == created.id),
        "actor {} still bound after unbind",
        created.id
    );

    delete_actor(&client, &created.id).expect("delete actor");
    guard.disarm();
}
