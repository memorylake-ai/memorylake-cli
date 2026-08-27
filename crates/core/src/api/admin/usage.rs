//! Quota and usage of the team (`/admin/v1/usage`).

use crate::client::Client;
use crate::error::Result;

use super::path;
use super::types::Usage;

/// Query parameters for a usage report.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct GetUsageParams {
    /// First day to include, `YYYY-MM-DD`. The server defaults it to six days
    /// before `end_date`.
    pub start_date: Option<String>,
    /// Last day to include, `YYYY-MM-DD`. The server defaults it to today in
    /// its own timezone.
    pub end_date: Option<String>,
}

/// Return the team's quota snapshot plus consumption over the requested
/// period (at most 92 days). Requires usage read permission on the whole
/// team.
pub fn get_usage(client: &Client, params: &GetUsageParams) -> Result<Usage> {
    let mut query = Vec::new();
    if let Some(start_date) = &params.start_date {
        query.push(("start_date", start_date.clone()));
    }
    if let Some(end_date) = &params.end_date {
        query.push(("end_date", end_date.clone()));
    }
    client.get_data(path::USAGE, &query)
}
