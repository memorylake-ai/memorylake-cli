//! Forget one fact in one scope
//! (`POST .../facts/{fact_id}/forget`).

use crate::client::Client;
use crate::error::{Error, Result};

use super::path::forget_path;
use super::types::FactScope;

/// Wire value of the API error code meaning the fact does not exist in the
/// addressed scope.
const NOT_FOUND_CODE: &str = "NOT_FOUND";

/// Forget one fact. Returns `false` when the fact does not exist in `scope`.
///
/// There is deliberately no batch variant. The API's batch forget endpoint
/// (`POST .../facts/forget` with `{"ids": [...]}`) is **not atomic**: it walks
/// the ids in order and stops at the first one it cannot find, answering
/// `NOT_FOUND` for the whole call — every id before that one has already been
/// forgotten, the rest have not, and the response names only the id it
/// stopped on (measured 2026-07-24, reproduced 2026-08-07 and 2026-10-09).
/// Forgetting per id keeps every outcome attributable; callers wanting bulk
/// behavior loop over this and report per-id results.
///
/// Forgetting an already-forgotten fact succeeds again; only an id that never
/// existed in the scope answers `NOT_FOUND` (measured 2026-08-07 and
/// 2026-10-09).
///
/// `NOT_FOUND` maps to `Ok(false)` rather than an error because facts are
/// strictly owned by one scope: a wrong-scope id is an expected outcome the
/// caller reports, not a failure that should abort the remaining ids.
pub fn forget_fact(
    client: &Client,
    workspace_id: &str,
    scope: &FactScope,
    fact_id: &str,
) -> Result<bool> {
    let path = forget_path(workspace_id, scope, fact_id);
    match client.post_data::<serde_json::Value, _>(&path, &serde_json::json!({})) {
        Ok(_) => Ok(true),
        Err(Error::Api {
            code: Some(code), ..
        }) if code == NOT_FOUND_CODE => Ok(false),
        Err(err) => Err(err),
    }
}
