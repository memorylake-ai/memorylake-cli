//! List workspaces (`GET /api/v3/workspaces`).

use serde::{Deserialize, Serialize};

use crate::client::Client;
use crate::error::Result;

use super::types::Workspace;

/// Paginated workspace list payload.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct WorkspaceList {
    /// Workspaces on this page.
    #[serde(default)]
    pub items: Vec<Workspace>,
    /// Token for the next page, if any.
    #[serde(default)]
    pub continuation_token: Option<String>,
}

/// Query parameters for listing workspaces.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct ListWorkspacesParams {
    /// Page size.
    pub page_size: Option<u32>,
    /// Continuation token from a previous page.
    pub continuation_token: Option<String>,
    /// Fuzzy filter by workspace name (partial match). Sent as `name_fuzzy`.
    pub name_fuzzy: Option<String>,
}

/// List workspaces visible to the authenticated caller.
pub fn list_workspaces(client: &Client, params: &ListWorkspacesParams) -> Result<WorkspaceList> {
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
    client.get_data("/api/v3/workspaces", &query)
}
