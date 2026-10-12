//! Quota and usage of the team (`/admin/v1/usage`).

use crate::client::Client;
use crate::error::Result;

use super::path;
use super::types::{Page, Usage, UsageRecord, UsageSummary, push_each, push_opt, push_page_query};

/// Query parameters for a usage report.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct GetUsageParams {
    /// First day to include, `YYYY-MM-DD`. The server defaults it to six days
    /// before `end_date`.
    pub start_date: Option<String>,
    /// Last day to include, `YYYY-MM-DD`. The server defaults it to today in
    /// its own timezone.
    pub end_date: Option<String>,
    /// Also return a per-day series (`bucket_width=1d`).
    pub daily: bool,
    /// Breakdowns to add: `member` (a per-member breakdown; needs permission
    /// to view the whole team's usage) and/or `model` (per-model breakdowns
    /// inside each day of the series). Sent as one repeated `group_by`
    /// parameter each.
    pub group_by: Vec<String>,
}

/// Longest period a usage report or a usage-record listing may span.
pub const MAX_USAGE_WINDOW_DAYS: i64 = 92;

/// Query parameters for listing usage records.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct ListUsageRecordsParams {
    /// Page size.
    pub page_size: Option<u32>,
    /// Continuation token from a previous page. Send the same filters with
    /// it.
    pub continuation_token: Option<String>,
    /// Earliest record time to include, Unix seconds. The server defaults it
    /// to 7 days before `end_time`.
    pub start_time: Option<i64>,
    /// Latest record time, Unix seconds (exclusive). The server defaults it
    /// to now. The window may span at most 92 days.
    pub end_time: Option<i64>,
    /// Only these models or platform operations.
    pub models: Vec<String>,
    /// `model_call` or `platform_task`; both when absent.
    pub kind: Option<String>,
    /// Only records of these members.
    pub principal_ids: Vec<String>,
    /// Only the record of this request id.
    pub request_id: Option<String>,
}

/// Return the team's quota snapshot plus consumption over the requested
/// period (at most 92 days). Callers allowed to view only their own usage get
/// just that, and `scope` says which.
pub fn get_usage(client: &Client, params: &GetUsageParams) -> Result<Usage> {
    let mut query = Vec::new();
    push_opt(&mut query, "start_date", params.start_date.as_ref());
    push_opt(&mut query, "end_date", params.end_date.as_ref());
    if params.daily {
        query.push(("bucket_width", "1d".to_string()));
    }
    push_each(&mut query, "group_by", &params.group_by);
    client.get_data(path::USAGE, &query)
}

/// Return consumption for today, the trailing 7 days, and the month to date.
pub fn get_usage_summary(client: &Client) -> Result<UsageSummary> {
    client.get_data(path::USAGE_SUMMARY, &[])
}

/// List individual consumption records, newest first. Records are near
/// real-time, while [`get_usage`] reads aggregates refreshed every few
/// minutes, so the two can briefly disagree.
pub fn list_usage_records(
    client: &Client,
    params: &ListUsageRecordsParams,
) -> Result<Page<UsageRecord>> {
    let mut query = Vec::new();
    push_page_query(
        &mut query,
        params.page_size,
        params.continuation_token.as_deref(),
    );
    push_opt(&mut query, "start_time", params.start_time.as_ref());
    push_opt(&mut query, "end_time", params.end_time.as_ref());
    push_each(&mut query, "model", &params.models);
    push_opt(&mut query, "kind", params.kind.as_ref());
    push_each(&mut query, "principal_id", &params.principal_ids);
    push_opt(&mut query, "request_id", params.request_id.as_ref());
    client.get_data(path::USAGE_RECORDS, &query)
}
