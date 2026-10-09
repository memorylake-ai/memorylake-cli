//! Update a database connection (`PATCH /api/v3/db-connections/{id}`).

use serde::Serialize;

use crate::client::Client;
use crate::error::Result;

use super::db_connection_path;
use super::types::{DbConnection, DbPassword};

/// Request body for updating a connection.
///
/// Partial-update semantics: only the keys present are changed, so every
/// field is skipped when `None`.
///
/// The password is never readable, so the server cannot re-check credentials
/// on its own: per the API, changing any credential field (`host`, `port`,
/// `username`, `database`) means sending `password` again. See
/// [`Self::changes_credentials`].
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize)]
pub struct UpdateDbConnectionRequest {
    /// New display name.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,
    /// New description.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    /// New host.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub host: Option<String>,
    /// New port, 1-65535.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub port: Option<u16>,
    /// New user name.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub username: Option<String>,
    /// Password, required alongside any other credential change.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub password: Option<DbPassword>,
    /// New database name.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub database: Option<String>,
}

impl UpdateDbConnectionRequest {
    /// Whether this changes a credential field other than the password.
    ///
    /// Such a change must carry the password too.
    pub fn changes_credentials(&self) -> bool {
        self.host.is_some()
            || self.port.is_some()
            || self.username.is_some()
            || self.database.is_some()
    }

    /// Whether the request would change nothing at all.
    pub fn is_empty(&self) -> bool {
        self.name.is_none()
            && self.description.is_none()
            && self.password.is_none()
            && !self.changes_credentials()
    }
}

/// Update a connection's fields; omitted fields stay unchanged.
pub fn update_db_connection(
    client: &Client,
    connection_id: &str,
    request: &UpdateDbConnectionRequest,
) -> Result<DbConnection> {
    client.patch_data(&db_connection_path(connection_id), request)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn omitted_fields_are_absent_from_the_body() {
        let body = serde_json::to_string(&UpdateDbConnectionRequest {
            name: Some("renamed".into()),
            ..Default::default()
        })
        .unwrap();
        assert_eq!(body, r#"{"name":"renamed"}"#);
    }

    #[test]
    fn a_credential_change_carries_the_password() {
        let body = serde_json::to_value(UpdateDbConnectionRequest {
            host: Some("new.example.com".into()),
            port: Some(5433),
            password: Some(DbPassword::new("s3cret")),
            ..Default::default()
        })
        .unwrap();
        assert_eq!(
            body,
            serde_json::json!({"host": "new.example.com", "port": 5433, "password": "s3cret"})
        );
    }

    #[test]
    fn credential_changes_are_recognized() {
        assert!(!UpdateDbConnectionRequest::default().changes_credentials());
        let rename = UpdateDbConnectionRequest {
            name: Some("x".into()),
            description: Some(String::new()),
            ..Default::default()
        };
        assert!(!rename.changes_credentials());
        for request in [
            UpdateDbConnectionRequest {
                host: Some("h".into()),
                ..Default::default()
            },
            UpdateDbConnectionRequest {
                port: Some(1),
                ..Default::default()
            },
            UpdateDbConnectionRequest {
                username: Some("u".into()),
                ..Default::default()
            },
            UpdateDbConnectionRequest {
                database: Some("d".into()),
                ..Default::default()
            },
        ] {
            assert!(request.changes_credentials(), "{request:?}");
        }
    }

    #[test]
    fn only_the_default_request_is_empty() {
        assert!(UpdateDbConnectionRequest::default().is_empty());
        assert!(
            !UpdateDbConnectionRequest {
                password: Some(DbPassword::new("p")),
                ..Default::default()
            }
            .is_empty()
        );
    }
}
