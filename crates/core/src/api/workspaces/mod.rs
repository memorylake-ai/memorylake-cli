//! Workspace v3 API (`/api/v3/workspaces`).

mod create;
mod get;
mod list;
mod types;
mod update;

pub use create::{CreateWorkspaceRequest, create_workspace};
pub use get::{get_workspace, get_workspace_by_custom_id};
pub use list::{ListWorkspacesParams, WorkspaceList, list_workspaces};
pub use types::Workspace;
pub use update::{
    UpdateWorkspaceRequest, WORKSPACE_DESCRIPTION_MAX_CHARS, WORKSPACE_NAME_MAX_CHARS,
    update_workspace,
};
