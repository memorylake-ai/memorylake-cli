//! List knowledge entries
//! (`GET /api/v3/workspaces/{workspace_id}/analysis-models/{model_id}/entries`).

use serde::{Deserialize, Serialize};

use crate::client::Client;
use crate::error::Result;

use super::path::entries_path;
use super::types::KnowledgeEntry;

/// Paginated knowledge entry list payload.
///
/// The API documents that this response never carries `total`; the field is
/// still modeled so it is printed if that ever changes.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct KnowledgeEntryList {
    /// Entries on this page.
    #[serde(default)]
    pub items: Vec<KnowledgeEntry>,
    /// Total across all pages, when the server can count it.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub total: Option<u64>,
    /// Token for the next page, if any.
    #[serde(default)]
    pub continuation_token: Option<String>,
}

/// Query parameters for listing knowledge entries.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ListEntriesParams {
    /// Which kind of knowledge to list. Required: a model lists one kind at a
    /// time, so reading all of it means listing each kind its template offers.
    pub entity_type: String,
    /// Rank entries by similarity to this text, closest first.
    ///
    /// A similarity search, not a filter: text matching nothing still returns
    /// the closest entries, so a non-empty page is not a hit — read `score`.
    /// Disabled entries are left out.
    pub keyword: Option<String>,
    /// Only entries from this origin: `MANUAL` (written by hand) or `BUILD`
    /// (produced when the model was built). Sent as `from`.
    pub origin: Option<String>,
    /// Only these entries, by id. Sent as one repeated `ids` parameter each.
    pub ids: Vec<String>,
    /// Page size, 1-100. The server defaults to 20.
    pub page_size: Option<u32>,
    /// Continuation token from a previous page.
    pub continuation_token: Option<String>,
}

impl ListEntriesParams {
    /// Parameters listing every entry of `entity_type`, with no other filter.
    pub fn new(entity_type: impl Into<String>) -> Self {
        Self {
            entity_type: entity_type.into(),
            keyword: None,
            origin: None,
            ids: Vec::new(),
            page_size: None,
            continuation_token: None,
        }
    }

    /// Render as client query pairs, omitting unset values.
    fn to_query(&self) -> Vec<(&'static str, String)> {
        let mut query = vec![("entity_type", self.entity_type.clone())];
        if let Some(keyword) = &self.keyword {
            query.push(("keyword", keyword.clone()));
        }
        if let Some(origin) = &self.origin {
            query.push(("from", origin.clone()));
        }
        for id in &self.ids {
            query.push(("ids", id.clone()));
        }
        if let Some(page_size) = self.page_size {
            query.push(("page_size", page_size.to_string()));
        }
        if let Some(token) = &self.continuation_token {
            query.push(("continuation_token", token.clone()));
        }
        query
    }
}

/// List one kind of knowledge held by an analysis model.
pub fn list_entries(
    client: &Client,
    workspace_id: &str,
    model_id: &str,
    params: &ListEntriesParams,
) -> Result<KnowledgeEntryList> {
    client.get_data(&entries_path(workspace_id, model_id), &params.to_query())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn entity_type_is_always_sent() {
        assert_eq!(
            ListEntriesParams::new("few_shot").to_query(),
            vec![("entity_type", "few_shot".to_string())]
        );
    }

    #[test]
    fn to_query_renders_every_param_and_repeats_ids() {
        let params = ListEntriesParams {
            keyword: Some("revenue".into()),
            origin: Some("MANUAL".into()),
            ids: vec!["e-1".into(), "e-2".into()],
            page_size: Some(5),
            continuation_token: Some("tok".into()),
            ..ListEntriesParams::new("biz_rule")
        };
        assert_eq!(
            params.to_query(),
            vec![
                ("entity_type", "biz_rule".to_string()),
                ("keyword", "revenue".to_string()),
                ("from", "MANUAL".to_string()),
                ("ids", "e-1".to_string()),
                ("ids", "e-2".to_string()),
                ("page_size", "5".to_string()),
                ("continuation_token", "tok".to_string()),
            ]
        );
    }

    #[test]
    fn list_decodes_the_documented_example_page() {
        let raw = r#"{
            "items": [{
                "id": "f03156c2b32311f18c28b638f7ce0c3d",
                "entity_type": "few_shot",
                "disabled": false,
                "score": 0.0,
                "payload": {"artifact": {"few_shot": {"type": "SQL", "content": "SELECT 1"}}}
            }],
            "continuation_token": "eyJsYXN0X2lkIjoiMiJ9"
        }"#;
        let list: KnowledgeEntryList = serde_json::from_str(raw).expect("decode page");
        assert_eq!(list.items.len(), 1);
        assert!(list.total.is_none());
        assert_eq!(
            list.continuation_token.as_deref(),
            Some("eyJsYXN0X2lkIjoiMiJ9")
        );
    }
}
