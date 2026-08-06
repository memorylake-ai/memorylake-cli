//! List projects (`GET /api/v3/workspaces/{workspace_id}/projects`).

use serde::{Deserialize, Serialize};

use crate::client::Client;
use crate::error::Result;

use super::path::projects_path;
use super::types::Project;

/// Paginated project list payload.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ProjectList {
    /// Projects on this page.
    #[serde(default)]
    pub items: Vec<Project>,
    /// Token for the next page, if any.
    #[serde(default)]
    pub continuation_token: Option<String>,
}

/// Query parameters for listing projects.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct ListProjectsParams {
    /// Page size.
    pub page_size: Option<u32>,
    /// Continuation token from a previous page.
    pub continuation_token: Option<String>,
    /// Fuzzy filter by project name (partial match). Sent as `name_fuzzy`.
    pub name_fuzzy: Option<String>,
}

/// List the projects inside `workspace_id`.
pub fn list_projects(
    client: &Client,
    workspace_id: &str,
    params: &ListProjectsParams,
) -> Result<ProjectList> {
    let mut query = Vec::new();
    if let Some(page_size) = params.page_size {
        query.push(("page_size", page_size.to_string()));
    }
    if let Some(token) = &params.continuation_token {
        query.push(("continuation_token", token.clone()));
    }
    if let Some(name_fuzzy) = &params.name_fuzzy {
        query.push(("name_fuzzy", name_fuzzy.clone()));
    }
    client.get_data(&projects_path(workspace_id), &query)
}
