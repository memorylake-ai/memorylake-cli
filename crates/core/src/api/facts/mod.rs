//! Facts v3 API
//! (`.../actors/{id}/facts`, `.../projects/{id}/memories/facts`, and the
//! workspace-wide listing at `.../memories/facts`).
//!
//! A fact is one atomic remembered statement. Facts are strictly owned: each
//! one lives under exactly one scope — an actor or a project — and every
//! operation names that scope explicitly ([`FactScope`]). Unlike documents,
//! facts have no processing pipeline: a stored fact is searchable immediately.
//!
//! Facts are immutable. There is no update endpoint — the server handles
//! semantic conflicts between facts itself, so "updating" is simply storing
//! the new statement with [`add_facts`].

mod add;
mod forget;
mod list;
mod path;
mod types;

pub use add::{AddFactsRequest, AddedFacts, add_facts};
pub use forget::forget_fact;
pub use list::{FactList, ListFactsParams, list_facts};
pub use types::{Fact, FactOwner, FactScope};
