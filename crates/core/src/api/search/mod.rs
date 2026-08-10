//! Search API (`/api/v3/workspaces/{workspace_id}/memories/search`).
//!
//! Natural-language retrieval across one workspace. Unlike the resource
//! families in this crate, search is a single `POST` with a filter body: it has
//! no pagination, and it answers with two independent result sets — matched
//! documents and matched facts — rather than one uniform list.

mod memories;
mod types;

pub use memories::{SearchMemoriesRequest, search_memories};
pub use types::{
    DocumentItem, MEMORY_TYPE_DOCUMENT, MEMORY_TYPE_FACT, MemoryType, SearchDocument, SearchFact,
    SearchResults,
};
