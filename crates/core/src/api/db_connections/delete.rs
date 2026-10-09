//! Delete a database connection (`DELETE /api/v3/db-connections/{id}`).

use crate::client::Client;
use crate::error::Result;

use super::db_connection_path;

/// Delete a connection.
///
/// Refused while any datasource still reads through it
/// ([`DbConnection::datasource_count`](super::DbConnection::datasource_count)
/// is non-zero). An unknown id is `NOT_FOUND` (measured 2026-10-09).
pub fn delete_db_connection(client: &Client, connection_id: &str) -> Result<()> {
    client.delete_empty(&db_connection_path(connection_id))
}
