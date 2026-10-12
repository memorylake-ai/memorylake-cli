//! `memorylake usage` commands.

use anyhow::{Context, Result, bail};
use clap::{Args, Subcommand};
use memorylake_core::api::admin::{
    GetUsageParams, ListUsageRecordsParams, MAX_USAGE_WINDOW_DAYS, get_usage, get_usage_summary,
    list_usage_records,
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
    #[arg(long, value_parser = parse_date)]
    start_date: Option<String>,
    /// Last day to include, YYYY-MM-DD. Defaults to today.
    #[arg(long, value_parser = parse_date)]
    end_date: Option<String>,
    /// Also return a per-day series (`buckets`), idle days included.
    #[arg(long)]
    daily: bool,
    /// Add a breakdown: `member` (per member; needs permission to view the
    /// whole team's usage) or `model` (per model inside each day of
    /// --daily; the per-model totals for the whole period are always
    /// returned, so without --daily it changes nothing). Repeat, or separate
    /// with commas, for both.
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
        /// Number of items per page (1-100).
        #[arg(long, value_parser = clap::value_parser!(u32).range(1..=100))]
        page_size: Option<u32>,
        /// Continuation token from a previous response. Pass the same
        /// filters with it.
        #[arg(long)]
        continuation_token: Option<String>,
        /// Earliest record time, Unix seconds (inclusive). Defaults to 7
        /// days before --end-time.
        #[arg(long, value_parser = clap::value_parser!(i64).range(1..))]
        start_time: Option<i64>,
        /// Latest record time, Unix seconds (exclusive). Defaults to now.
        /// The window may span at most 92 days.
        #[arg(long, value_parser = clap::value_parser!(i64).range(1..))]
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
    // A window the server would refuse is a caller mistake; say so before
    // asking for credentials.
    if let Some(UsageCommand::Records {
        start_time: Some(start),
        end_time: Some(end),
        ..
    }) = &args.command
    {
        check_window(*start, *end)?;
    }

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

/// `--start-time` must come before `--end-time`, at most 92 days apart.
fn check_window(start: i64, end: i64) -> Result<()> {
    if start >= end {
        bail!("--start-time ({start}) must be before --end-time ({end})");
    }
    if end - start > MAX_USAGE_WINDOW_DAYS * 86_400 {
        bail!(
            "--start-time and --end-time are more than {MAX_USAGE_WINDOW_DAYS} days apart; \
             list the records in several windows"
        );
    }
    Ok(())
}

/// A calendar day as the usage report takes it: YYYY-MM-DD.
fn parse_date(raw: &str) -> std::result::Result<String, String> {
    let bytes = raw.as_bytes();
    let shaped = bytes.len() == 10
        && bytes.iter().enumerate().all(|(i, b)| match i {
            4 | 7 => *b == b'-',
            _ => b.is_ascii_digit(),
        });
    let month = raw.get(5..7).and_then(|m| m.parse::<u8>().ok());
    let day = raw.get(8..10).and_then(|d| d.parse::<u8>().ok());
    if !shaped || !matches!(month, Some(1..=12)) || !matches!(day, Some(1..=31)) {
        return Err(format!("expected a date as YYYY-MM-DD, got `{raw}`"));
    }
    Ok(raw.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn dates_must_be_calendar_days() {
        assert_eq!(parse_date("2026-10-01").as_deref(), Ok("2026-10-01"));
        for bad in [
            "2026-1-01",
            "2026/10/01",
            "2026-13-01",
            "2026-10-00",
            "yesterday",
            "",
        ] {
            assert!(parse_date(bad).is_err(), "{bad}");
        }
    }

    #[test]
    fn a_record_window_is_ordered_and_bounded() {
        assert!(check_window(1, 2).is_ok());
        assert!(check_window(1, 1 + MAX_USAGE_WINDOW_DAYS * 86_400).is_ok());
        assert!(check_window(2, 2).is_err());
        assert!(check_window(1, 2 + MAX_USAGE_WINDOW_DAYS * 86_400).is_err());
    }
}
