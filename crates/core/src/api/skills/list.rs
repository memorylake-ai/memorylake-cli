//! List skills (`GET /api/v3/skills`).

use serde::{Deserialize, Serialize};

use crate::client::Client;
use crate::error::Result;

use super::SKILLS_PATH;
use super::types::Skill;

/// Paginated skill list payload.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SkillList {
    /// Skills on this page, newest first.
    #[serde(default)]
    pub items: Vec<Skill>,
    /// Total matching skills across all pages, when the server reports it.
    ///
    /// Absent when visibility is decided per row (and absent on every list
    /// measured 2026-10-09); page with `continuation_token` instead.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub total: Option<u64>,
    /// Token for the next page, if any.
    #[serde(default)]
    pub continuation_token: Option<String>,
}

/// Query parameters for listing skills.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct ListSkillsParams {
    /// Page size (1–100; the server defaults to 20).
    pub page_size: Option<u32>,
    /// Continuation token from a previous page.
    pub continuation_token: Option<String>,
    /// Fuzzy filter by skill name. Sent as `name_fuzzy`.
    pub name_fuzzy: Option<String>,
}

impl ListSkillsParams {
    /// Render the non-empty parameters as query pairs.
    fn to_query(&self) -> Vec<(&'static str, String)> {
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

/// List the skills the caller may open, newest first.
pub fn list_skills(client: &Client, params: &ListSkillsParams) -> Result<SkillList> {
    client.get_data(SKILLS_PATH, &params.to_query())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn to_query_omits_unset_parameters() {
        assert!(ListSkillsParams::default().to_query().is_empty());
    }

    #[test]
    fn to_query_renders_all_parameters() {
        let params = ListSkillsParams {
            page_size: Some(10),
            continuation_token: Some("tok".into()),
            name_fuzzy: Some("equity".into()),
        };
        assert_eq!(
            params.to_query(),
            vec![
                ("page_size", "10".to_string()),
                ("continuation_token", "tok".to_string()),
                ("name_fuzzy", "equity".to_string()),
            ]
        );
    }

    #[test]
    fn list_decodes_without_total_or_token() {
        // Measured 2026-10-09: the server omits both on a single page.
        let list: SkillList = serde_json::from_str(r#"{"items":[{"id":"skill-1"}]}"#).unwrap();
        assert_eq!(list.items.len(), 1);
        assert!(list.total.is_none());
        assert!(list.continuation_token.is_none());
    }
}
