//! Database connections v3 API (`/api/v3/db-connections`).
//!
//! A connection records how to reach one database and the credentials to read
//! it. It is the first link in the structured-data chain: a workspace
//! [datasource](crate::api::db_datasources) reads one schema through a
//! connection, and a project [database memory](crate::api::databases) reads a
//! datasource.
//!
//! Connections are not scoped to a workspace. The password is write-only: it
//! is sent on create, update, and connectivity test, and never returned by any
//! endpoint, which is why [`DbPassword`] hides its value from `Debug` and the
//! client redacts it from trace logs.

mod create;
mod delete;
mod get;
mod list;
mod list_schemas;
mod test_connectivity;
mod types;
mod update;

pub use create::{CreateDbConnectionRequest, create_db_connection};
pub use delete::delete_db_connection;
pub use get::{get_db_connection, get_db_connection_by_custom_id};
pub use list::{DbConnectionList, ListDbConnectionsParams, list_db_connections};
pub use list_schemas::list_db_connection_schemas;
pub use test_connectivity::{ConnectivityTestRequest, test_db_connectivity};
pub use types::{ConnectionSchemas, DbConnection, DbPassword};
pub use update::{UpdateDbConnectionRequest, update_db_connection};

use crate::api::path::encode_segment;

/// Connection collection endpoint.
///
/// Relative to the configured base URL, which already carries the
/// `/openapi/memorylake` prefix.
const DB_CONNECTIONS_PATH: &str = "/api/v3/db-connections";

/// `/api/v3/db-connections/{id}`
///
/// `id` is the server-assigned id, or a `custom_id` when the request also sets
/// `by_custom_id=true`.
fn db_connection_path(id: &str) -> String {
    format!("{DB_CONNECTIONS_PATH}/{}", encode_segment(id))
}

/// `/api/v3/db-connections/{id}/schemas`
fn db_connection_schemas_path(id: &str) -> String {
    format!("{}/schemas", db_connection_path(id))
}

/// `/api/v3/db-connections/connectivity-test`
fn connectivity_test_path() -> String {
    format!("{DB_CONNECTIONS_PATH}/connectivity-test")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn paths_are_relative_to_the_base_url() {
        assert_eq!(
            db_connection_path("2810c96aaa8711f1b11d1e97653d06ca"),
            "/api/v3/db-connections/2810c96aaa8711f1b11d1e97653d06ca"
        );
        assert_eq!(
            db_connection_schemas_path("conn-1"),
            "/api/v3/db-connections/conn-1/schemas"
        );
        assert_eq!(
            connectivity_test_path(),
            "/api/v3/db-connections/connectivity-test"
        );
    }

    #[test]
    fn ids_cannot_escape_their_path_segment() {
        // `connectivity-test` is a sibling route; an id must never be able to
        // reach it, or anything else, by smuggling in a separator.
        assert_eq!(
            db_connection_path("../connectivity-test"),
            "/api/v3/db-connections/..%2Fconnectivity-test"
        );
        assert_eq!(
            db_connection_schemas_path("a?b#c"),
            "/api/v3/db-connections/a%3Fb%23c/schemas"
        );
    }
}
