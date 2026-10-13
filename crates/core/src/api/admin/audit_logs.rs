//! The team's audit log (`/admin/v1/audit-logs`).

use serde::{Deserialize, Serialize};
use serde_json::{Map, Value};

use crate::client::Client;
use crate::error::Result;

use super::path;
use super::types::{Page, push_each, push_opt, push_page_query};

/// Longest window one audit-log query may span, in seconds (92 days).
pub const MAX_AUDIT_WINDOW_SECS: i64 = 92 * 24 * 60 * 60;

/// Most actions one listing may filter on.
pub const MAX_AUDIT_ACTIONS: usize = 50;

/// Most actors one listing may filter on.
pub const MAX_AUDIT_ACTORS: usize = 100;

/// Longest target type a listing may filter on, in characters.
pub const MAX_AUDIT_TARGET_TYPE: usize = 32;

/// Longest target id a listing may filter on, in characters.
pub const MAX_AUDIT_TARGET_ID: usize = 128;

/// One audit event.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AuditLogEntry {
    /// Event id.
    pub id: String,
    /// When it happened, Unix seconds.
    pub created_at: i64,
    /// `management` or `system`.
    pub category: String,
    /// What happened, as `resource:verb`, e.g. `member:change_role`.
    pub action: String,
    /// Who did it: `type` is `user`, `api_key` (with `api_key_id`), or
    /// `system`.
    pub actor: Value,
    /// `succeeded`, `failed`, or `partial`.
    pub result: String,
    /// What it was done to, when anything.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub target: Option<Value>,
    /// Fields the CLI does not model yet (reason, IP, request id).
    #[serde(flatten)]
    pub extra: Map<String, Value>,
}

/// Query parameters for listing audit events.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct ListAuditLogsParams {
    /// Page size, 1–100.
    pub page_size: Option<u32>,
    /// Continuation token from a previous page. Send the same filters with
    /// it.
    pub continuation_token: Option<String>,
    /// `management` or `system`; both when absent.
    pub category: Option<String>,
    /// Only these actions.
    pub actions: Vec<String>,
    /// Only events performed by these members.
    pub actor_ids: Vec<String>,
    /// Only events on this kind of target, e.g. `principal`.
    pub target_type: Option<String>,
    /// Only events on this target.
    pub target_id: Option<String>,
    /// `succeeded`, `failed`, or `partial`.
    pub result: Option<String>,
    /// Earliest event time, Unix seconds (inclusive). The server defaults it
    /// to 7 days before `end_time`.
    pub start_time: Option<i64>,
    /// Latest event time, Unix seconds (exclusive). Defaults to now.
    pub end_time: Option<i64>,
}

/// List audit events, newest first. There is no total: page with the
/// continuation token until it is absent. Rate-limited separately (20 a
/// minute per key by default).
pub fn list_audit_logs(
    client: &Client,
    params: &ListAuditLogsParams,
) -> Result<Page<AuditLogEntry>> {
    let mut query = Vec::new();
    push_page_query(
        &mut query,
        params.page_size,
        params.continuation_token.as_deref(),
    );
    push_opt(&mut query, "category", params.category.as_ref());
    push_each(&mut query, "action", &params.actions);
    push_each(&mut query, "actor_id", &params.actor_ids);
    push_opt(&mut query, "target_type", params.target_type.as_ref());
    push_opt(&mut query, "target_id", params.target_id.as_ref());
    push_opt(&mut query, "result", params.result.as_ref());
    push_opt(&mut query, "start_time", params.start_time.as_ref());
    push_opt(&mut query, "end_time", params.end_time.as_ref());
    client.get_data(path::AUDIT_LOGS, &query)
}
