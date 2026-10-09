//! Start an index build
//! (`POST /api/v3/workspaces/{workspace_id}/db-datasources/{datasource_id}/build`).

use serde_json::Value;

use crate::client::Client;
use crate::error::Result;

use super::path::db_datasource_child_path;

/// Start building a new index over the schemas a datasource covers.
///
/// Asynchronous: returns once the build is accepted. Poll
/// [`super::get_db_datasource`] until [`super::DbDatasource::is_building`]
/// turns false. Unlike a document reload this is not a retry: every call
/// produces a new build version, and it may be run again at any time.
///
/// The endpoint takes no body and answers with an empty envelope; an empty
/// JSON object is sent and whatever `data` holds is discarded, while a failed
/// envelope is still an error.
pub fn build_db_datasource(client: &Client, workspace_id: &str, datasource_id: &str) -> Result<()> {
    client.post_data::<Value, _>(
        &db_datasource_child_path(workspace_id, datasource_id, "build"),
        &serde_json::json!({}),
    )?;
    Ok(())
}
