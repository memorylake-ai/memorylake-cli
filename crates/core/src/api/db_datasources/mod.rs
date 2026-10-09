//! Database datasources v3 API
//! (`/api/v3/workspaces/{workspace_id}/db-datasources`).
//!
//! A datasource is one schema of a [connection](crate::api::db_connections),
//! brought into a workspace and indexed there. Project
//! [database memories](crate::api::databases) read through a datasource.
//!
//! The connection and the schema are fixed at creation. Building the index is
//! asynchronous: creating a datasource starts the first build, and
//! [`build_db_datasource`] starts another; in both cases poll
//! [`get_db_datasource`] until [`DbDatasource::is_building`] turns false.

mod build;
mod create;
mod delete;
mod get;
mod list;
mod list_columns;
mod list_tables;
mod path;
mod types;
mod update;
mod update_schema_metadata;

pub use build::build_db_datasource;
pub use create::{CreateDbDatasourceRequest, create_db_datasource};
pub use delete::delete_db_datasource;
pub use get::{get_db_datasource, get_db_datasource_by_custom_id};
pub use list::{DbDatasourceList, ListDbDatasourcesParams, list_db_datasources};
pub use list_columns::{DbColumnList, list_db_datasource_columns};
pub use list_tables::{DbTableList, list_db_datasource_tables};
pub use types::{DbColumn, DbDatasource, DbTable};
pub use update::{UpdateDbDatasourceRequest, update_db_datasource};
pub use update_schema_metadata::{
    MAX_SCHEMA_METADATA_EDITS, SchemaMetadataEdit, UpdateSchemaMetadataRequest,
    update_db_datasource_schema_metadata,
};
