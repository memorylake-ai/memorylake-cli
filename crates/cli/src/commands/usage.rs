//! `memorylake usage` commands.

use anyhow::{Context, Result};
use clap::{Args, Subcommand};
use memorylake_core::api::admin::{
    GetUsageParams, ListUsageRecordsParams, get_usage, get_usage_summary, list_usage_records,
};

use super::{api_client, print_json};

/// Show the team's quota snapshot and consumption over a period.
///
/// The quota is a snapshot taken now; the totals cover the requested period
/// (at most 92 days, default the last 7 days). Without permission to view
/// the whole team's usage you get your own, and `scope` says which.
#[derive(Debug, Args)]
#[command(args_conflicts_with_subcommands = true)]
pub struct UsageArgs {
    #[command(subcommand)]
    command: Option<UsageCommand>,
    #[command(flatten)]
    report: ReportArgs,
}

/// Flags of the bare `usage` report.
#[derive(Debug, Args)]
struct ReportArgs {
    /// First day to include, YYYY-MM-DD. Defaults to six days before the end.
    #[arg(long)]
    start_date: Option<String>,
    /// Last day to include, YYYY-MM-DD. Defaults to today.
    #[arg(long)]
    end_date: Option<String>,
    /// Also return a per-day series (`buckets`), idle days included.
    #[arg(long)]
    daily: bool,
    /// Add a breakdown: `member` (per member; needs permission to view the
    /// whole team's usage) or `model` (per model inside each day of
    /// --daily). Repeat, or separate with commas, for both.
    #[arg(long, value_delimiter = ',', value_parser = ["member", "model"])]
    group_by: Vec<String>,
}

/// Usage subcommands.
#[derive(Debug, Subcommand)]
enum UsageCommand {
    /// Consumption today, over the trailing 7 days, and month to date.
    Summary,
    /// List individual consumption records, newest first: one per billed
    /// model request or platform operation.
    ///
    /// Records are near real-time; the `usage` report reads aggregates
    /// refreshed every few minutes, so the two can briefly disagree.
    Records {
        /// Number of items per page.
        #[arg(long)]
        page_size: Option<u32>,
        /// Continuation token from a previous response. Pass the same
        /// filters with it.
        #[arg(long)]
        continuation_token: Option<String>,
        /// Earliest record time, Unix seconds (inclusive). Defaults to 7
        /// days before --end-time.
        #[arg(long)]
        start_time: Option<i64>,
        /// Latest record time, Unix seconds (exclusive). Defaults to now.
        /// The window may span at most 92 days.
        #[arg(long)]
        end_time: Option<i64>,
        /// Only this model or platform operation (files_process,
        /// memory_input, retrieval_call, search_call). Repeat, or separate
        /// with commas, for several.
        #[arg(long = "model", value_delimiter = ',')]
        models: Vec<String>,
        /// Only model requests or only platform operations.
        #[arg(long, value_parser = ["model_call", "platform_task"])]
        kind: Option<String>,
        /// Only records of this member. Repeat, or separate with commas, for
        /// several. Callers limited to their own records may pass only
        /// their own id.
        #[arg(long = "member", value_name = "PRINCIPAL_ID", value_delimiter = ',')]
        principal_ids: Vec<String>,
        /// Only the record of this request id.
        #[arg(long)]
        request_id: Option<String>,
    },
}

/// Execute the `usage` command.
pub fn run(args: UsageArgs, profile: Option<String>, base_url: Option<String>) -> Result<()> {
    let client = api_client(profile, base_url)?;

    match args.command {
        None => {
            let ReportArgs {
                start_date,
                end_date,
                daily,
                group_by,
            } = args.report;
            let data = get_usage(
                &client,
                &GetUsageParams {
                    start_date,
                    end_date,
                    daily,
                    group_by,
                },
            )
            .context("get usage")?;
            print_json(&data)
        }
        Some(UsageCommand::Summary) => {
            let data = get_usage_summary(&client).context("get usage summary")?;
            print_json(&data)
        }
        Some(UsageCommand::Records {
            page_size,
            continuation_token,
            start_time,
            end_time,
            models,
            kind,
            principal_ids,
            request_id,
        }) => {
            let data = list_usage_records(
                &client,
                &ListUsageRecordsParams {
                    page_size,
                    continuation_token,
                    start_time,
                    end_time,
                    models,
                    kind,
                    principal_ids,
                    request_id,
                },
            )
            .context("list usage records")?;
            print_json(&data)
        }
    }
}
