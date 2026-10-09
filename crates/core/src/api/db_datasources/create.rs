//! Create a datasource
//! (`POST /api/v3/workspaces/{workspace_id}/db-datasources`).

use serde::Serialize;

use crate::client::Client;
use crate::error::Result;

use super::path::db_datasources_path;
use super::types::DbDatasource;

/// Request body for creating a datasource.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct CreateDbDatasourceRequest {
    /// Schemas to cover. Exactly one for now: the API takes a list so that
    /// raising the cap later is additive, and rejects anything else with
    /// `INVALID_ARGUMENT` (measured 2026-10-09). Each must be readable through
    /// the connection; immutable afterwards.
    pub schemas: Vec<String>,
    /// Display name.
    pub name: String,
    /// Free-form description.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    /// Connection to read through. Requires `dbconn:read` on it.
    pub db_connection_id: String,
    /// Caller-defined id, unique within the tenant and immutable once set.
    /// At most 255 characters; a value starting with `_sys_` is rejected.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub custom_id: Option<String>,
    /// Regular expression selecting which tables are indexed, passed to the
    /// indexer unchanged. The server defaults to `.*`.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub table_filter_rule: Option<String>,
}

/// Create a datasource over one schema of an existing connection.
///
/// Creating it also starts the first index build, so the returned datasource
/// is usually [building](DbDatasource::is_building).
pub fn create_db_datasource(
    client: &Client,
    workspace_id: &str,
    request: &CreateDbDatasourceRequest,
) -> Result<DbDatasource> {
    client.post_data(&db_datasources_path(workspace_id), request)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn required_fields_only_serialize_without_optional_keys() {
        let body = serde_json::to_value(CreateDbDatasourceRequest {
            schemas: vec!["public".into()],
            name: "sales".into(),
            description: None,
            db_connection_id: "conn-1".into(),
            custom_id: None,
            table_filter_rule: None,
        })
        .unwrap();
        assert_eq!(
            body,
            serde_json::json!({
                "schemas": ["public"],
                "name": "sales",
                "db_connection_id": "conn-1"
            })
        );
    }

    #[test]
    fn optional_fields_serialize_when_set() {
        let body = serde_json::to_value(CreateDbDatasourceRequest {
            schemas: vec!["public".into()],
            name: "sales".into(),
            description: Some("orders".into()),
            db_connection_id: "conn-1".into(),
            custom_id: Some("ds-1".into()),
            table_filter_rule: Some("^orders_.*".into()),
        })
        .unwrap();
        assert_eq!(body["description"], "orders");
        assert_eq!(body["custom_id"], "ds-1");
        assert_eq!(body["table_filter_rule"], "^orders_.*");
    }
}
