//! Create a database connection (`POST /api/v3/db-connections`).

use serde::Serialize;

use crate::client::Client;
use crate::error::Result;

use super::DB_CONNECTIONS_PATH;
use super::types::{DbConnection, DbPassword};

/// Request body for creating a connection.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct CreateDbConnectionRequest {
    /// Display name.
    pub name: String,
    /// Free-form description.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    /// Server host name or address.
    pub host: String,
    /// Server port, 1-65535. The server defaults to 5432.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub port: Option<u16>,
    /// User name to authenticate with.
    pub username: String,
    /// Password to authenticate with. Write-only.
    pub password: DbPassword,
    /// Name of the database to read.
    pub database: String,
    /// Caller-defined id, unique within the tenant and immutable once set.
    /// At most 255 characters; a value starting with `_sys_` is rejected.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub custom_id: Option<String>,
}

/// Save a connection to a database.
///
/// Production status, measured 2026-10-09: this endpoint answers HTTP 500
/// `INTERNAL_ERROR` both for an unreachable database and for a reachable
/// public read-only Postgres whose credentials pass
/// [`super::test_db_connectivity`]. The request shape here follows the
/// published spec and is pinned by wire tests; no success response could be
/// observed. When a create fails, the connectivity test is the way to tell a
/// credential problem (`DB_CONNECTION_FAILED`) from a server-side one.
pub fn create_db_connection(
    client: &Client,
    request: &CreateDbConnectionRequest,
) -> Result<DbConnection> {
    client.post_data(DB_CONNECTIONS_PATH, request)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn request() -> CreateDbConnectionRequest {
        CreateDbConnectionRequest {
            name: "analytics".into(),
            description: None,
            host: "db.example.com".into(),
            port: None,
            username: "ro".into(),
            password: DbPassword::new("s3cret"),
            database: "analytics".into(),
            custom_id: None,
        }
    }

    #[test]
    fn required_fields_only_serialize_without_optional_keys() {
        assert_eq!(
            serde_json::to_value(request()).unwrap(),
            serde_json::json!({
                "name": "analytics",
                "host": "db.example.com",
                "username": "ro",
                "password": "s3cret",
                "database": "analytics"
            })
        );
    }

    #[test]
    fn optional_fields_serialize_when_set() {
        let body = serde_json::to_value(CreateDbConnectionRequest {
            description: Some("replica".into()),
            port: Some(6543),
            custom_id: Some("conn-1".into()),
            ..request()
        })
        .unwrap();
        assert_eq!(body["description"], "replica");
        assert_eq!(body["port"], 6543);
        assert_eq!(body["custom_id"], "conn-1");
    }

    #[test]
    fn debug_output_does_not_carry_the_password() {
        let rendered = format!("{:?}", request());
        assert!(!rendered.contains("s3cret"), "{rendered}");
    }
}
