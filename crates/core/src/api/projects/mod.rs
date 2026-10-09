//! Project v3 API (`/api/v3/workspaces/{workspace_id}/projects`).
//!
//! A project is a knowledge container inside a workspace; it organizes
//! documents, conversations, and extracted facts. Every endpoint here is
//! addressed relative to the workspace that owns the project.

mod create;
mod delete;
mod get;
mod list;
mod path;
mod statistics;
mod types;
mod update;

pub use create::{CreateProjectRequest, create_project};
pub use delete::delete_project;
pub use get::{get_project, get_project_by_custom_id};
pub use list::{ListProjectsParams, ProjectList, list_projects};
pub use statistics::{ProjectStatistics, StatusSummary, get_project_statistics};
pub use types::{Industry, Project};
pub use update::{UpdateProjectRequest, update_project};
