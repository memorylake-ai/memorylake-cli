//! Memory conflicts (`.../{actors|projects|agents}/{id}/memories/conflicts`).
//!
//! The server compares a scope's facts with each other — and, for projects,
//! with its documents — and records each contradiction it finds as a
//! conflict for review. Resolving one may forget or rewrite the facts
//! involved. The deprecated `/api/v2/projects/{id}/memories/conflicts`
//! endpoints are deliberately not bound.

mod get;
mod list;
mod resolve;
mod types;

pub use get::get_conflict;
pub use list::{ConflictList, ListConflictsParams, MAX_CONFLICT_PAGE_SIZE, list_conflicts};
pub use resolve::{ConflictResolution, ConflictResolveResult, resolve_conflict};
pub use types::{
    ConflictCategory, ConflictFactEdit, ConflictFactSnapshot, ConflictFileChunk,
    ConflictResolutionRecord, ConflictType, MemoryConflict,
};
