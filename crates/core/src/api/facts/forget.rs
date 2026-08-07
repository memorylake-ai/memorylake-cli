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
/// There is deliberately no batch variant. The API's batch forget endpoint is
/// **not atomic**: given a mix of valid and invalid ids it deletes the valid
/// ones and then fails the whole call (measured 2026-07-24, reproduced
/// 2026-08-07), leaving the caller unable to tell what was removed. Forgetting
/// per id keeps every outcome attributable; callers wanting bulk behavior loop
/// over this and report per-id results.
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
