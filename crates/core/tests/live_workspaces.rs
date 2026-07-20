//! Live integration tests against a real MemoryLake environment.
//!
//! Requires `MEMORYLAKE_API_KEY` (from the environment or repo-root `.env`).
//! When the key is missing, these tests skip so CI without secrets still passes.

use std::path::PathBuf;
use std::time::{SystemTime, UNIX_EPOCH};

use memorylake_core::api::workspaces::{
    CreateWorkspaceRequest, ListWorkspacesParams, create_workspace, list_workspaces,
};
use memorylake_core::{Client, DEFAULT_BASE_URL, ENV_API_KEY, ENV_BASE_URL};

fn load_dotenv() {
    let manifest_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let candidates = [
        manifest_dir.join("../../.env"),
        manifest_dir.join(".env"),
        PathBuf::from(".env"),
    ];
    for path in candidates {
        if path.is_file() {
            let _ = dotenvy::from_path(&path);
            break;
        }
    }
}

fn live_client() -> Option<Client> {
    load_dotenv();
    let api_key = std::env::var(ENV_API_KEY).ok().filter(|s| !s.is_empty())?;
    let base_url = std::env::var(ENV_BASE_URL)
        .ok()
        .filter(|s| !s.is_empty())
        .unwrap_or_else(|| DEFAULT_BASE_URL.to_string());
    Some(Client::new(base_url, api_key).expect("build live client"))
}

#[test]
fn refresh_list_workspaces_page() {
    let Some(client) = live_client() else {
        eprintln!("skipping live test: {ENV_API_KEY} not set");
        return;
    };

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
    let Some(client) = live_client() else {
        eprintln!("skipping live test: {ENV_API_KEY} not set");
        return;
    };

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
