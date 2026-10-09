//! Search API (`/api/v3/workspaces/{workspace_id}/memories/search`).
//!
//! Natural-language retrieval across one workspace. Unlike the resource
//! families in this crate, search is a single `POST` with a filter body: it has
//! no pagination, and it answers with one ranked list split into per-type
//! collections — documents, facts, databases — whose entries carry a shared
//! `rank` to merge them back by.

mod memories;
mod types;

pub use memories::{SearchMemoriesRequest, search_memories};
pub use types::{
    DocumentItem, Highlight, HighlightChunk, MEMORY_TYPE_DATABASE, MEMORY_TYPE_DOCUMENT,
    MEMORY_TYPE_FACT, MemoryType, SearchDocument, SearchFact, SearchResults,
};
