//! Live integration tests against a real MemoryLake environment.
//!
//! Requires `MEMORYLAKE_API_KEY` (from the environment or repo-root `.env`).
//! Missing or empty key fails the tests (no skip).

mod common;

use std::time::{SystemTime, UNIX_EPOCH};

use memorylake_core::api::workspaces::{
    CreateWorkspaceRequest, ListWorkspacesParams, create_workspace, list_workspaces,
};

use common::live_client;

#[test]
fn refresh_list_workspaces_page() {
    let client = live_client();

    let page = list_workspaces(
        &client,
        &ListWorkspacesParams {
            page_size: Some(1),
            ..ListWorkspacesParams::default()
        },
    )
    .expect("list workspaces with page_size=1");

    // A successful envelope decode is enough for refresh-equivalent coverage.
    let _ = page.items;
}

#[test]
fn create_and_list_workspace() {
    let client = live_client();

    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("clock")
        .as_nanos();
    let custom_id = format!("cli-live-{nanos}");
    let name = format!("CLI Live Workspace {nanos}");

    let created = create_workspace(
        &client,
        &CreateWorkspaceRequest {
            name: name.clone(),
            custom_id: custom_id.clone(),
            description: Some("created by memorylake-cli live test".into()),
        },
    )
    .expect("create workspace");

    assert!(!created.id.is_empty());
    assert_eq!(created.name, name);
    assert_eq!(created.custom_id.as_deref(), Some(custom_id.as_str()));

    let listed = list_workspaces(
        &client,
        &ListWorkspacesParams {
            page_size: Some(50),
            name: Some(name.clone()),
            ..ListWorkspacesParams::default()
        },
    )
    .expect("list workspaces after create");

    assert!(
        listed.items.iter().any(|ws| ws.id == created.id),
        "created workspace {} not found in list results",
        created.id
    );
}
