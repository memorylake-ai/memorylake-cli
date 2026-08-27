//! `memorylake usage` command.

use anyhow::{Context, Result};
use clap::Args;
use memorylake_core::api::admin::{GetUsageParams, get_usage};

use super::{api_client, print_json};

/// Show the team's quota snapshot and consumption over a period.
///
/// The quota is a snapshot taken now; the totals cover the requested period
/// (at most 92 days, default the last 7 days).
#[derive(Debug, Args)]
pub struct UsageArgs {
    /// First day to include, YYYY-MM-DD. Defaults to six days before the end.
    #[arg(long)]
    start_date: Option<String>,
    /// Last day to include, YYYY-MM-DD. Defaults to today.
    #[arg(long)]
    end_date: Option<String>,
}

/// Execute the `usage` command.
pub fn run(args: UsageArgs, profile: Option<String>, base_url: Option<String>) -> Result<()> {
    let client = api_client(profile, base_url)?;
    let data = get_usage(
        &client,
        &GetUsageParams {
            start_date: args.start_date,
            end_date: args.end_date,
        },
    )
    .context("get usage")?;
    print_json(&data)
}
