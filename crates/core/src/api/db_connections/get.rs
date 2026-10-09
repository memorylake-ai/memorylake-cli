//! Get a database connection (`GET /api/v3/db-connections/{id}`).

use crate::client::Client;
use crate::error::Result;

use super::db_connection_path;
use super::types::DbConnection;

/// Fetch a connection by its server-assigned id.
pub fn get_db_connection(client: &Client, connection_id: &str) -> Result<DbConnection> {
    client.get_data(&db_connection_path(connection_id), &[])
}

/// Fetch a connection by its caller-defined `custom_id`.
///
/// Only lookup takes `by_custom_id`; update, delete, and schema listing
/// address a connection by its server-assigned id.
pub fn get_db_connection_by_custom_id(client: &Client, custom_id: &str) -> Result<DbConnection> {
    client.get_data(
        &db_connection_path(custom_id),
        &[("by_custom_id", "true".to_string())],
    )
}
