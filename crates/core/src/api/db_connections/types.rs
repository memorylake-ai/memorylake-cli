//! Shared database-connection resource types.

use serde::de::IgnoredAny;
use serde::{Deserialize, Serialize, Serializer};
use serde_json::{Map, Value};

/// A database password on its way to the server.
///
/// Serializes as the bare string the API expects, but never shows its value in
/// `Debug` output, so a request struct logged with `{:?}` cannot leak it.
#[derive(Clone, PartialEq, Eq)]
pub struct DbPassword(String);

impl DbPassword {
    /// Wrap a password.
    pub fn new(password: impl Into<String>) -> Self {
        Self(password.into())
    }

    /// The password itself. Only for putting it on the wire.
    pub fn expose(&self) -> &str {
        &self.0
    }
}

impl std::fmt::Debug for DbPassword {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("DbPassword(<redacted>)")
    }
}

impl Serialize for DbPassword {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        serializer.serialize_str(&self.0)
    }
}

/// A saved database connection.
///
/// Every field is optional, as in the published schema, and fields this build
/// does not model are kept in [`Self::extra`] so printing a connection never
/// drops data. The one exception is `password`: the API documents it as
/// write-only, but should a server ever echo it back, it is swallowed here
/// rather than carried into `extra` and printed.
///
/// Not `Eq`: the swallowed field is a `serde::de::IgnoredAny`, which is only
/// `PartialEq`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct DbConnection {
    /// Server-assigned connection id. Opaque; do not parse it.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub id: Option<String>,
    /// Display name.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,
    /// Free-form description.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    /// Server host name or address.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub host: Option<String>,
    /// Server port.
    #[serde(
        default,
        deserialize_with = "crate::api::lenient::int",
        skip_serializing_if = "Option::is_none"
    )]
    pub port: Option<i64>,
    /// User name the connection authenticates with.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub username: Option<String>,
    /// Name of the database to read.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub database: Option<String>,
    /// Caller-supplied identifier for external reference.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub custom_id: Option<String>,
    /// How many datasources read through this connection. Deleting the
    /// connection is refused while this is non-zero.
    #[serde(
        default,
        deserialize_with = "crate::api::lenient::int",
        skip_serializing_if = "Option::is_none"
    )]
    pub datasource_count: Option<i64>,
    /// User or agent that created the connection.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub created_by: Option<String>,
    /// Creation timestamp (ISO 8601).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub created_at: Option<String>,
    /// Last update timestamp (ISO 8601).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub updated_at: Option<String>,
    /// A `password` the server should never send. Decoded only so that it does
    /// not land in [`Self::extra`]; never serialized.
    #[serde(default, rename = "password", skip_serializing)]
    echoed_password: Option<IgnoredAny>,
    /// Fields returned by the server that this client does not model.
    #[serde(flatten)]
    pub extra: Map<String, Value>,
}

/// Schemas a connection can read.
///
/// Returned both by the connectivity test and by the saved-connection schema
/// listing; the entries are the choices for a datasource's `schemas`.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct ConnectionSchemas {
    /// Readable schema names.
    #[serde(default)]
    pub schemas: Vec<String>,
    /// Fields returned by the server that this client does not model.
    #[serde(flatten)]
    pub extra: Map<String, Value>,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn password_debug_output_is_redacted() {
        let password = DbPassword::new("hunter2-secret");
        let rendered = format!("{password:?}");
        assert!(!rendered.contains("hunter2"), "{rendered}");
    }

    #[test]
    fn password_serializes_as_a_bare_string() {
        assert_eq!(
            serde_json::to_string(&DbPassword::new("p@ss")).unwrap(),
            r#""p@ss""#
        );
    }

    #[test]
    fn documented_connection_shape_deserializes() {
        let connection: DbConnection = serde_json::from_str(
            r#"{
                "id": "2810c96aaa8711f1b11d1e97653d06ca",
                "name": "analytics",
                "description": "read replica",
                "host": "db.internal.example.com",
                "port": 5432,
                "username": "analytics_ro",
                "database": "analytics",
                "custom_id": "my-connection-001",
                "datasource_count": 3,
                "created_by": "user::1",
                "created_at": "2026-04-01T00:00:00Z",
                "updated_at": "2026-04-02T00:00:00Z"
            }"#,
        )
        .expect("deserialize documented shape");

        assert_eq!(connection.port, Some(5432));
        assert_eq!(connection.datasource_count, Some(3));
        assert!(connection.extra.is_empty());
    }

    #[test]
    fn a_sparse_connection_still_decodes() {
        let connection: DbConnection = serde_json::from_str("{}").expect("decode empty object");
        assert!(connection.id.is_none());
        assert!(connection.port.is_none());
    }

    #[test]
    fn an_odd_count_or_port_does_not_fail_a_list_page() {
        let list: super::super::DbConnectionList = serde_json::from_str(
            r#"{"items":[{"id":"c1","port":70000,"datasource_count":-1},
                         {"id":"c2","port":"5432","datasource_count":1.5}],
                "total":"2"}"#,
        )
        .expect("odd numbers must not fail the page");
        assert_eq!(list.items[0].port, Some(70_000));
        assert_eq!(list.items[0].datasource_count, Some(-1));
        assert_eq!(list.items[1].port, Some(5432));
        assert_eq!(list.items[1].datasource_count, None);
        assert_eq!(list.total, Some(2));
    }

    #[test]
    fn unknown_fields_survive_a_round_trip() {
        let connection: DbConnection =
            serde_json::from_str(r#"{"id":"c1","ssl_mode":"require"}"#).expect("decode");
        let rendered = serde_json::to_value(&connection).expect("serialize");
        assert_eq!(rendered["ssl_mode"], "require");
    }

    #[test]
    fn an_echoed_password_is_never_printed() {
        // Write-only by contract. If a server ever breaks that, the CLI prints
        // whatever it decoded, so the value must not reach `extra`.
        let connection: DbConnection =
            serde_json::from_str(r#"{"id":"c1","password":"hunter2-secret"}"#).expect("decode");
        let rendered = serde_json::to_string(&connection).expect("serialize");
        assert!(!rendered.contains("hunter2"), "{rendered}");
        assert!(!connection.extra.contains_key("password"));
    }

    #[test]
    fn schemas_decode_and_tolerate_absence() {
        let schemas: ConnectionSchemas =
            serde_json::from_str(r#"{"schemas":["public","sales"]}"#).expect("decode");
        assert_eq!(schemas.schemas, vec!["public", "sales"]);
        let empty: ConnectionSchemas = serde_json::from_str("{}").expect("decode empty");
        assert!(empty.schemas.is_empty());
    }
}
