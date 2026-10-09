//! Get a datasource
//! (`GET /api/v3/workspaces/{workspace_id}/db-datasources/{datasource_id}`).

use crate::client::Client;
use crate::error::Result;

use super::path::db_datasource_path;
use super::types::DbDatasource;

/// Fetch a datasource, including its schemas and build state.
///
/// This is how a build is observed: poll until
/// [`DbDatasource::is_building`] turns false.
pub fn get_db_datasource(
    client: &Client,
    workspace_id: &str,
    datasource_id: &str,
) -> Result<DbDatasource> {
    client.get_data(&db_datasource_path(workspace_id, datasource_id), &[])
}

/// Fetch a datasource by its caller-defined `custom_id`.
///
/// Only lookup takes `by_custom_id`; every other datasource endpoint addresses
/// it by its server-assigned id.
pub fn get_db_datasource_by_custom_id(
    client: &Client,
    workspace_id: &str,
    custom_id: &str,
) -> Result<DbDatasource> {
    client.get_data(
        &db_datasource_path(workspace_id, custom_id),
        &[("by_custom_id", "true".to_string())],
    )
}
