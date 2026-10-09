//! List skill versions (`GET /api/v3/skills/{id}/versions`).

use serde::{Deserialize, Serialize};

use crate::client::Client;
use crate::error::Result;

use super::skill_versions_path;
use super::types::SkillVersion;

/// Paginated skill version list payload.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SkillVersionList {
    /// Versions on this page, newest first.
    #[serde(default)]
    pub items: Vec<SkillVersion>,
    /// Total versions across all pages, when the server reports it.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub total: Option<u64>,
    /// Token for the next page, if any.
    #[serde(default)]
    pub continuation_token: Option<String>,
}

/// Query parameters for listing skill versions.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct ListSkillVersionsParams {
    /// Page size (1–100; the server defaults to 20).
    pub page_size: Option<u32>,
    /// Continuation token from a previous page.
    pub continuation_token: Option<String>,
}

impl ListSkillVersionsParams {
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

/// List the published versions of a skill, newest first.
pub fn list_skill_versions(
    client: &Client,
    id: &str,
    params: &ListSkillVersionsParams,
) -> Result<SkillVersionList> {
    client.get_data(&skill_versions_path(id), &params.to_query())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn to_query_omits_unset_parameters() {
        assert!(ListSkillVersionsParams::default().to_query().is_empty());
    }

    #[test]
    fn to_query_renders_pagination() {
        let params = ListSkillVersionsParams {
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

    #[test]
    fn list_keeps_the_reported_total() {
        let list: SkillVersionList =
            serde_json::from_str(r#"{"items":[{"version":1}],"total":1}"#).unwrap();
        assert_eq!(list.total, Some(1));
        assert_eq!(serde_json::to_value(&list).unwrap()["total"], 1);
    }
}
