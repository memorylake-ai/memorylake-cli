//! Shared datasource resource types.

use serde::{Deserialize, Serialize};
use serde_json::{Map, Value};

/// A database datasource: one schema of a connection, indexed in a workspace.
///
/// Every field is optional, as in the published schema; unmodeled fields are
/// kept in [`Self::extra`].
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct DbDatasource {
    /// Server-assigned datasource id. Opaque; do not parse it.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub id: Option<String>,
    /// Display name.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,
    /// Free-form description.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    /// Schemas covered. Exactly one for now.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub schemas: Option<Vec<String>>,
    /// Owning workspace.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub workspace_id: Option<String>,
    /// Connection this datasource reads through.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub db_connection_id: Option<String>,
    /// Caller-supplied identifier for external reference.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub custom_id: Option<String>,
    /// Regular expression selecting which tables are indexed; `.*` by default.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub table_filter_rule: Option<String>,
    /// How many database memories use this datasource. Deleting it is refused
    /// while any does.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub database_count: Option<u32>,
    /// Most recently built index; empty until the first build finishes.
    #[serde(
        default,
        deserialize_with = "version_string",
        skip_serializing_if = "Option::is_none"
    )]
    pub build_version: Option<String>,
    /// Index currently being built; empty when none is.
    #[serde(
        default,
        deserialize_with = "version_string",
        skip_serializing_if = "Option::is_none"
    )]
    pub building_version: Option<String>,
    /// Version of the underlying data as last reported. Often empty, which
    /// says nothing about freshness; when set and different from
    /// `build_version`, a rebuild is due.
    #[serde(
        default,
        deserialize_with = "version_string",
        skip_serializing_if = "Option::is_none"
    )]
    pub data_version: Option<String>,
    /// User or agent that created the datasource.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub created_by: Option<String>,
    /// Creation timestamp (ISO 8601).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub created_at: Option<String>,
    /// Last update timestamp (ISO 8601).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub updated_at: Option<String>,
    /// Fields returned by the server that this client does not model.
    #[serde(flatten)]
    pub extra: Map<String, Value>,
}

/// Decode a build or data version, accepting a bare number as well as the
/// documented string.
///
/// The published example writes a version as a bare 17-digit number even
/// though the field is typed string. No datasource could be created to check
/// what production sends, so both are taken rather than letting one field
/// fail the whole response. A number is kept as its exact digits.
fn version_string<'de, D>(deserializer: D) -> Result<Option<String>, D::Error>
where
    D: serde::Deserializer<'de>,
{
    match Option::<Value>::deserialize(deserializer)? {
        None | Some(Value::Null) => Ok(None),
        Some(Value::String(version)) => Ok(Some(version)),
        Some(Value::Number(version)) => Ok(Some(version.to_string())),
        Some(other) => Err(serde::de::Error::custom(format!(
            "expected a version string, got {other}"
        ))),
    }
}

impl DbDatasource {
    /// Whether an index build is running.
    ///
    /// Per the API, a non-empty `building_version` is the whole definition of
    /// "building"; its going from set to empty means the build finished.
    pub fn is_building(&self) -> bool {
        self.building_version
            .as_deref()
            .is_some_and(|version| !version.is_empty())
    }

    /// Whether any index build has ever finished.
    pub fn has_been_built(&self) -> bool {
        self.build_version
            .as_deref()
            .is_some_and(|version| !version.is_empty())
    }
}

/// A table a datasource covers.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct DbTable {
    /// Editable annotation describing what the table holds.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub comment: Option<String>,
    /// Schema the table belongs to.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub schema_name: Option<String>,
    /// Table name.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub table_name: Option<String>,
    /// Kind of relation, as the database reports it.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub table_type: Option<String>,
    /// Fields returned by the server that this client does not model.
    #[serde(flatten)]
    pub extra: Map<String, Value>,
}

/// A column of one table a datasource covers.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct DbColumn {
    /// Editable annotation describing what the column holds.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub comment: Option<String>,
    /// Schema the column belongs to.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub schema_name: Option<String>,
    /// Table the column belongs to.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub table_name: Option<String>,
    /// Column name.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub column_name: Option<String>,
    /// Column type, as the database reports it.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub data_type: Option<String>,
    /// Whether the column's values are indexed for semantic matching.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub embedding_enabled: Option<bool>,
    /// Upper bound on how many of the column's values are indexed.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub max_embedding_rows: Option<u64>,
    /// Fields returned by the server that this client does not model.
    #[serde(flatten)]
    pub extra: Map<String, Value>,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn documented_datasource_shape_deserializes() {
        let datasource: DbDatasource = serde_json::from_str(
            r#"{
                "id": "ds-1",
                "name": "sales",
                "description": "",
                "schemas": ["public"],
                "workspace_id": "ws-1",
                "db_connection_id": "conn-1",
                "custom_id": "my-datasource-001",
                "table_filter_rule": ".*",
                "database_count": 3,
                "build_version": "20260401023011680",
                "building_version": "",
                "data_version": "",
                "created_by": "user::1",
                "created_at": "2026-04-01T00:00:00Z",
                "updated_at": "2026-04-01T00:00:00Z"
            }"#,
        )
        .expect("deserialize documented shape");

        assert_eq!(
            datasource.schemas.as_deref(),
            Some(&["public".to_string()][..])
        );
        assert_eq!(datasource.database_count, Some(3));
        assert!(!datasource.is_building());
        assert!(datasource.has_been_built());
        assert!(datasource.extra.is_empty());
    }

    #[test]
    fn building_is_a_non_empty_building_version() {
        let building: DbDatasource =
            serde_json::from_str(r#"{"building_version":"20261009"}"#).unwrap();
        assert!(building.is_building());
        assert!(!building.has_been_built());

        for idle in [
            r#"{}"#,
            r#"{"building_version":""}"#,
            r#"{"building_version":null}"#,
        ] {
            let datasource: DbDatasource = serde_json::from_str(idle).unwrap();
            assert!(!datasource.is_building(), "{idle}");
        }
    }

    #[test]
    fn a_numeric_build_version_is_kept_rather_than_failing_the_response() {
        // The published example writes the version as a bare number even
        // though the field is typed string.
        let datasource: DbDatasource =
            serde_json::from_str(r#"{"id":"ds-1","build_version":20260401023011680}"#)
                .expect("a numeric version must not fail the response");
        assert_eq!(
            datasource.build_version.as_deref(),
            Some("20260401023011680")
        );
        assert!(datasource.has_been_built());
    }

    #[test]
    fn a_version_of_another_kind_is_rejected() {
        assert!(serde_json::from_str::<DbDatasource>(r#"{"build_version":[1]}"#).is_err());
    }

    #[test]
    fn tables_and_columns_decode_sparse_and_full() {
        let table: DbTable = serde_json::from_str(
            r#"{"comment":"orders","schema_name":"public","table_name":"orders","table_type":"table"}"#,
        )
        .unwrap();
        assert_eq!(table.table_name.as_deref(), Some("orders"));

        let column: DbColumn = serde_json::from_str(
            r#"{"schema_name":"public","table_name":"orders","column_name":"order_status",
                "data_type":"varchar","embedding_enabled":true,"max_embedding_rows":10000}"#,
        )
        .unwrap();
        assert_eq!(column.embedding_enabled, Some(true));
        assert_eq!(column.max_embedding_rows, Some(10_000));

        assert!(serde_json::from_str::<DbColumn>("{}").is_ok());
        assert!(serde_json::from_str::<DbTable>("{}").is_ok());
    }
}
