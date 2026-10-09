//! Test credentials before saving them
//! (`POST /api/v3/db-connections/connectivity-test`).

use serde::Serialize;

use crate::client::Client;
use crate::error::Result;

use super::connectivity_test_path;
use super::types::{ConnectionSchemas, DbPassword};

/// Credentials to try.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct ConnectivityTestRequest {
    /// Server host name or address.
    pub host: String,
    /// Server port, 1-65535. The server defaults to 5432.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub port: Option<u16>,
    /// User name to authenticate with.
    pub username: String,
    /// Password to authenticate with.
    pub password: DbPassword,
    /// Name of the database to read.
    pub database: String,
}

/// Check unsaved credentials and report the schemas they can read.
///
/// Nothing is saved. Measured on production 2026-10-09: a refused port or an
/// unresolvable host fails fast with HTTP 400 `DB_CONNECTION_FAILED`; an
/// address that drops packets is only given up on by the server after about
/// 30 seconds, as HTTP 500 `INTERNAL_ERROR` — which this client's own 30
/// second timeout usually reports first.
pub fn test_db_connectivity(
    client: &Client,
    request: &ConnectivityTestRequest,
) -> Result<ConnectionSchemas> {
    client.post_data(&connectivity_test_path(), request)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn request_serializes_to_the_documented_body() {
        let body = serde_json::to_value(ConnectivityTestRequest {
            host: "127.0.0.1".into(),
            port: Some(5432),
            username: "u".into(),
            password: DbPassword::new("p"),
            database: "d".into(),
        })
        .unwrap();
        assert_eq!(
            body,
            serde_json::json!({
                "host": "127.0.0.1",
                "port": 5432,
                "username": "u",
                "password": "p",
                "database": "d"
            })
        );
    }

    #[test]
    fn an_unset_port_is_left_to_the_server_default() {
        let body = serde_json::to_value(ConnectivityTestRequest {
            host: "h".into(),
            port: None,
            username: "u".into(),
            password: DbPassword::new("p"),
            database: "d".into(),
        })
        .unwrap();
        assert!(body.get("port").is_none(), "{body}");
    }
}
