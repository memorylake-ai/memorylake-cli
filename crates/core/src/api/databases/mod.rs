//! Project database memories v3 API
//! (`/api/v3/workspaces/{workspace_id}/projects/{project_id}/memories/databases`).
//!
//! A database memory makes a [datasource](crate::api::db_datasources) of the
//! project's workspace available to the project as a memory source, optionally
//! with an `instruction` saying how questions against it should be answered.
//! The chain is connection → datasource → database memory.

mod create;
mod delete;
mod generate_instruction;
mod get;
mod list;
mod path;
mod reload;
mod types;
mod update;

pub use create::{CreateDatabaseMemoryRequest, create_database_memory};
pub use delete::delete_database_memory;
pub use generate_instruction::{InstructionDraft, generate_database_instruction};
pub use get::get_database_memory;
pub use list::{DatabaseMemoryList, ListDatabaseMemoriesParams, list_database_memories};
pub use reload::reload_database_memory;
pub use types::DatabaseMemory;
pub use update::{UpdateDatabaseMemoryRequest, update_database_memory};
