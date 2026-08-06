//! List agents (`GET /api/v3/agents`).

use serde::{Deserialize, Serialize};

use crate::client::Client;
use crate::error::Result;

use super::AGENTS_PATH;
use super::types::Agent;

/// Paginated agent list payload.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AgentList {
    /// Agents on this page.
    #[serde(default)]
    pub items: Vec<Agent>,
    /// Token for the next page, if any.
    #[serde(default)]
    pub continuation_token: Option<String>,
}

/// Query parameters for listing agents.
///
/// Also used for listing the agents bound to a workspace, which accepts the
/// same three parameters.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct ListAgentsParams {
    /// Page size.
    pub page_size: Option<u32>,
    /// Continuation token from a previous page.
    pub continuation_token: Option<String>,
    /// Fuzzy filter by agent name (partial match). Sent as `name_fuzzy`.
    pub name_fuzzy: Option<String>,
}

impl ListAgentsParams {
    /// Render the non-empty parameters as query pairs.
    pub(super) fn to_query(&self) -> Vec<(&'static str, String)> {
        let mut query = Vec::new();
        if let Some(page_size) = self.page_size {
            query.push(("page_size", page_size.to_string()));
        }
        if let Some(token) = &self.continuation_token {
            query.push(("continuation_token", token.clone()));
        }
        if let Some(name_fuzzy) = &self.name_fuzzy {
            query.push(("name_fuzzy", name_fuzzy.clone()));
        }
        query
    }
}

/// List agents visible to the authenticated caller.
pub fn list_agents(client: &Client, params: &ListAgentsParams) -> Result<AgentList> {
    client.get_data(AGENTS_PATH, &params.to_query())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn to_query_omits_unset_parameters() {
        assert!(ListAgentsParams::default().to_query().is_empty());
    }

    #[test]
    fn to_query_renders_all_parameters() {
        let params = ListAgentsParams {
            page_size: Some(25),
            continuation_token: Some("tok".into()),
            name_fuzzy: Some("sup".into()),
        };
        assert_eq!(
            params.to_query(),
            vec![
                ("page_size", "25".to_string()),
                ("continuation_token", "tok".to_string()),
                ("name_fuzzy", "sup".to_string()),
            ]
        );
    }
}
