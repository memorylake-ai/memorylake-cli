//! List the schemas a saved connection can read
//! (`GET /api/v3/db-connections/{id}/schemas`).

use crate::client::Client;
use crate::error::Result;

use super::db_connection_schemas_path;
use super::types::ConnectionSchemas;

/// List the schemas reachable through a saved connection.
///
/// These are the choices for a datasource's `schemas`. Unlike
/// [`super::test_db_connectivity`], this needs no password: the server uses
/// the stored credentials.
pub fn list_db_connection_schemas(
    client: &Client,
    connection_id: &str,
) -> Result<ConnectionSchemas> {
    client.get_data(&db_connection_schemas_path(connection_id), &[])
}
