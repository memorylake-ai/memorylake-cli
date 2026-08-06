//! List agent versions (`GET /api/v3/agents/{id}/versions`).

use serde::{Deserialize, Serialize};

use crate::client::Client;
use crate::error::Result;

use super::agent_versions_path;
use super::types::AgentVersion;

/// Paginated agent version list payload.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AgentVersionList {
    /// Versions on this page.
    #[serde(default)]
    pub items: Vec<AgentVersion>,
    /// Token for the next page, if any.
    #[serde(default)]
    pub continuation_token: Option<String>,
}

/// Query parameters for listing agent versions.
///
/// Unlike the agent list, this endpoint has no name filter.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct ListAgentVersionsParams {
    /// Page size.
    pub page_size: Option<u32>,
    /// Continuation token from a previous page.
    pub continuation_token: Option<String>,
}

impl ListAgentVersionsParams {
    /// Render the non-empty parameters as query pairs.
    fn to_query(&self) -> Vec<(&'static str, String)> {
        let mut query = Vec::new();
        if let Some(page_size) = self.page_size {
            query.push(("page_size", page_size.to_string()));
        }
        if let Some(token) = &self.continuation_token {
            query.push(("continuation_token", token.clone()));
        }
        query
    }
}

/// List the configuration versions of an agent, newest first.
pub fn list_agent_versions(
    client: &Client,
    id: &str,
    params: &ListAgentVersionsParams,
) -> Result<AgentVersionList> {
    client.get_data(&agent_versions_path(id), &params.to_query())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn to_query_omits_unset_parameters() {
        assert!(ListAgentVersionsParams::default().to_query().is_empty());
    }

    #[test]
    fn to_query_renders_pagination() {
        let params = ListAgentVersionsParams {
            page_size: Some(5),
            continuation_token: Some("tok".into()),
        };
        assert_eq!(
            params.to_query(),
            vec![
                ("page_size", "5".to_string()),
                ("continuation_token", "tok".to_string()),
            ]
        );
    }
}
