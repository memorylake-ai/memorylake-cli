//! Update a datasource
//! (`PATCH /api/v3/workspaces/{workspace_id}/db-datasources/{datasource_id}`).

use serde::Serialize;

use crate::client::Client;
use crate::error::Result;

use super::path::db_datasource_path;
use super::types::DbDatasource;

/// Request body for updating a datasource.
///
/// Only the name, description, and table filter can change; the connection
/// and schemas are fixed at creation. Unset fields are left out of the body
/// and stay as they are.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize)]
pub struct UpdateDbDatasourceRequest {
    /// New display name.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,
    /// New description.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    /// New table filter regular expression.
    ///
    /// Takes effect on the next [build](super::build_db_datasource).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub table_filter_rule: Option<String>,
}

impl UpdateDbDatasourceRequest {
    /// Whether the request would change nothing at all.
    pub fn is_empty(&self) -> bool {
        self.name.is_none() && self.description.is_none() && self.table_filter_rule.is_none()
    }
}

/// Update a datasource's name, description, or table filter.
pub fn update_db_datasource(
    client: &Client,
    workspace_id: &str,
    datasource_id: &str,
    request: &UpdateDbDatasourceRequest,
) -> Result<DbDatasource> {
    client.patch_data(&db_datasource_path(workspace_id, datasource_id), request)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn omitted_fields_are_absent_from_the_body() {
        let body = serde_json::to_string(&UpdateDbDatasourceRequest {
            table_filter_rule: Some("^orders$".into()),
            ..Default::default()
        })
        .unwrap();
        assert_eq!(body, r#"{"table_filter_rule":"^orders$"}"#);
    }

    #[test]
    fn an_explicit_empty_description_is_sent() {
        let body = serde_json::to_string(&UpdateDbDatasourceRequest {
            description: Some(String::new()),
            ..Default::default()
        })
        .unwrap();
        assert_eq!(body, r#"{"description":""}"#);
    }

    #[test]
    fn only_the_default_request_is_empty() {
        assert!(UpdateDbDatasourceRequest::default().is_empty());
        assert!(
            !UpdateDbDatasourceRequest {
                name: Some("n".into()),
                ..Default::default()
            }
            .is_empty()
        );
    }
}
