//! List a project's documents
//! (`GET /api/v3/workspaces/{workspace_id}/projects/{project_id}/memories/documents`).

use serde::{Deserialize, Serialize};

use crate::client::Client;
use crate::error::Result;

use super::path::documents_path;
use super::types::Document;

/// Paginated document list payload.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct DocumentList {
    /// Documents on this page.
    #[serde(default)]
    pub items: Vec<Document>,
    /// Token for the next page, if any.
    #[serde(default)]
    pub continuation_token: Option<String>,
}

/// Query parameters for listing documents.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct ListDocumentsParams {
    /// Page size. The server defaults to 20.
    pub page_size: Option<u32>,
    /// Continuation token from a previous page.
    pub continuation_token: Option<String>,
    /// Fuzzy filter by document name (partial match). Sent as `name_fuzzy`.
    pub name_fuzzy: Option<String>,
}

impl ListDocumentsParams {
    /// Render as client query pairs, omitting unset values.
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

/// List the documents imported into `project_id`.
///
/// Paging is not performed automatically: pass the returned
/// [`continuation_token`](DocumentList::continuation_token) back to fetch the
/// next page.
pub fn list_documents(
    client: &Client,
    workspace_id: &str,
    project_id: &str,
    params: &ListDocumentsParams,
) -> Result<DocumentList> {
    client.get_data(
        &documents_path(workspace_id, project_id),
        &params.to_query(),
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn to_query_omits_unset_params() {
        assert!(ListDocumentsParams::default().to_query().is_empty());
    }

    #[test]
    fn to_query_renders_every_param() {
        let params = ListDocumentsParams {
            page_size: Some(50),
            continuation_token: Some("token-abc".to_string()),
            name_fuzzy: Some("report".to_string()),
        };
        assert_eq!(
            params.to_query(),
            vec![
                ("page_size", "50".to_string()),
                ("continuation_token", "token-abc".to_string()),
                ("name_fuzzy", "report".to_string()),
            ]
        );
    }

    #[test]
    fn list_decodes_a_final_page_without_a_token() {
        let list: DocumentList = serde_json::from_str(
            r#"{"items":[{"id":"doc-1","name":"a.pdf","status":"okay"}],
                "continuation_token":null}"#,
        )
        .expect("decode final page");
        assert_eq!(list.items.len(), 1);
        assert!(list.continuation_token.is_none());
    }

    #[test]
    fn list_decodes_an_empty_page() {
        let list: DocumentList = serde_json::from_str("{}").expect("decode empty page");
        assert!(list.items.is_empty());
        assert!(list.continuation_token.is_none());
    }
}
