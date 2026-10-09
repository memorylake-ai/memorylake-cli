//! Facts v3 API
//! (`.../actors/{id}/facts`, `.../projects/{id}/memories/facts`,
//! `.../agents/{id}/facts`, and the workspace-wide listing at
//! `.../memories/facts`), plus each scope's memory [`conflicts`] and memory
//! [`settings`].
//!
//! A fact is one atomic remembered statement. Facts are strictly owned: each
//! one lives under exactly one scope — an actor, a project, or an agent — and
//! every operation names that scope explicitly ([`FactScope`]). Unlike
//! documents, facts have no processing pipeline: a stored fact is searchable
//! immediately.
//!
//! A fact's text and metadata can be edited in place with [`update_fact`];
//! every add, edit, and forget is recorded in its history ([`trace_fact`]).
//! The server also watches for contradictions between facts and records them
//! as [`conflicts`] for review.

pub mod conflicts;
pub mod settings;

mod add;
mod forget;
mod get;
mod list;
mod path;
mod trace;
mod types;
mod update;

pub use add::{AddFactsRequest, AddedFacts, add_facts};
pub use forget::forget_fact;
pub use get::get_fact;
pub use list::{FactList, ListFactsParams, MAX_FACT_LIST_OWNERS, MAX_FACT_PAGE_SIZE, list_facts};
pub use trace::{FactTrace, FactTraceEntry, trace_fact};
pub use types::{Fact, FactOwner, FactScope};
pub use update::{UpdateFactRequest, update_fact};
