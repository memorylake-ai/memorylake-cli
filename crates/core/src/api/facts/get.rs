//! Read one fact in one scope (`GET .../facts/{fact_id}`).

use crate::client::Client;
use crate::error::Result;

use super::path::fact_path;
use super::types::{Fact, FactScope};

/// Fetch one fact by id.
///
/// The id must belong to `scope`: a fact that exists under another scope, or
/// has been forgotten, answers `NOT_FOUND` (measured 2026-10-09). A forgotten
/// fact can still be read through [`trace_fact`](super::trace_fact).
pub fn get_fact(
    client: &Client,
    workspace_id: &str,
    scope: &FactScope,
    fact_id: &str,
) -> Result<Fact> {
    client.get_data(&fact_path(workspace_id, scope, fact_id), &[])
}
