//! Live integration tests for the project v3 API.
//!
//! Requires `MEMORYLAKE_API_KEY` (from the environment or repo-root `.env`).
//! Missing or empty key fails the tests (no skip).

mod common;

use std::time::{SystemTime, UNIX_EPOCH};

use memorylake_core::Client;
use memorylake_core::api::projects::{
    CreateProjectRequest, ListProjectsParams, UpdateProjectRequest, create_project, delete_project,
    get_project, get_project_by_custom_id, list_projects, update_project,
};
use memorylake_core::api::workspaces::{CreateWorkspaceRequest, create_workspace};

use common::live_client;

fn nanos() -> u128 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("clock")
        .as_nanos()
}

/// Create a workspace to host the projects under test and return its id.
///
/// The workspace itself is left behind: `workspace delete` is not implemented.
fn host_workspace(client: &Client, label: &str) -> String {
    let stamp = nanos();
    create_workspace(
        client,
        &CreateWorkspaceRequest {
            name: format!("Core Live {label} {stamp}"),
            custom_id: format!("core-live-{label}-{stamp}"),
            description: Some("created by memorylake-core project live test".into()),
        },
    )
    .expect("create host workspace")
    .id
}

#[test]
fn create_list_get_update_delete_round_trip() {
    let client = live_client();
    let workspace_id = host_workspace(&client, "projects");

    let stamp = nanos();
    let custom_id = format!("core-proj-{stamp}");
    let name = format!("Core Project {stamp}");
    let description = "created by memorylake-core project live test";

    let created = create_project(
        &client,
        &workspace_id,
        &CreateProjectRequest {
            name: name.clone(),
            custom_id: custom_id.clone(),
            description: Some(description.into()),
        },
    )
    .expect("create project");

    assert!(!created.id.is_empty());
    assert_eq!(created.name, name);
    assert_eq!(created.custom_id.as_deref(), Some(custom_id.as_str()));

    let listed = list_projects(
        &client,
        &workspace_id,
        &ListProjectsParams {
            page_size: Some(50),
            name_fuzzy: Some(name.clone()),
            ..ListProjectsParams::default()
        },
    )
    .expect("list projects after create");
    assert!(
        listed.items.iter().any(|p| p.id == created.id),
        "created project {} not found in list results",
        created.id
    );

    let by_id = get_project(&client, &workspace_id, &created.id).expect("get by id");
    assert_eq!(by_id.id, created.id);
    assert_eq!(by_id.name, name);

    let by_custom =
        get_project_by_custom_id(&client, &workspace_id, &custom_id).expect("get by custom_id");
    assert_eq!(by_custom.id, created.id);

    // Only `name` is sent, so the stored description must survive.
    let renamed = format!("{name} Renamed");
    let updated = update_project(
        &client,
        &workspace_id,
        &created.id,
        &UpdateProjectRequest {
            name: Some(renamed.clone()),
            description: None,
        },
    )
    .expect("update project");
    assert_eq!(updated.id, created.id);
    assert_eq!(updated.name, renamed);
    assert_eq!(
        updated.description.as_deref(),
        Some(description),
        "an omitted field must not clear the stored value"
    );

    delete_project(&client, &workspace_id, &created.id).expect("delete project");

    let after_delete = get_project(&client, &workspace_id, &created.id);
    assert!(
        after_delete.is_err(),
        "expected an error fetching a deleted project, got {after_delete:?}"
    );
}

#[test]
fn get_unknown_project_is_an_error() {
    let client = live_client();
    let workspace_id = host_workspace(&client, "unknown");

    let missing = get_project(&client, &workspace_id, "proj-does-not-exist-000000000000");
    assert!(
        missing.is_err(),
        "expected error for unknown project id, got {missing:?}"
    );
}
