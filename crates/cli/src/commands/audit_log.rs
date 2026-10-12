//! `memorylake audit-log` commands.

use anyhow::{Context, Result, bail};
use clap::Subcommand;
use memorylake_core::api::admin::{ListAuditLogsParams, MAX_AUDIT_WINDOW_SECS, list_audit_logs};

use super::{api_client, parse_non_blank, print_json};

/// Audit-log subcommands.
#[derive(Debug, Subcommand)]
pub enum AuditLogCommand {
    /// List the team's audit events, newest first: changes people and keys
    /// made (category management) and events the platform recorded itself
    /// (category system). Changes made with an API key name the key.
    ///
    /// The window defaults to the last 7 days and spans at most 92. Pages
    /// carry no total: follow the continuation token, with the same filters,
    /// until it is gone. Without permission to read the whole team's log you
    /// see only your own events. Limited to 20 requests a minute per key.
    List {
        /// Only `management` or only `system` events.
        #[arg(long, value_parser = ["management", "system"])]
        category: Option<String>,
        /// Only this action, e.g. member:change_role. Repeat, or separate
        /// with commas, for several.
        #[arg(long = "action", value_delimiter = ',', value_parser = parse_non_blank)]
        actions: Vec<String>,
        /// Only events performed by this member (principal id). Repeat, or
        /// separate with commas, for several.
        #[arg(long = "actor", value_name = "PRINCIPAL_ID", value_delimiter = ',', value_parser = parse_non_blank)]
        actor_ids: Vec<String>,
        /// Only events on this kind of target, e.g. principal.
        #[arg(long)]
        target_type: Option<String>,
        /// Only events on this target id.
        #[arg(long)]
        target_id: Option<String>,
        /// Only events with this outcome.
        #[arg(long, value_parser = ["succeeded", "failed", "partial"])]
        result: Option<String>,
        /// Earliest event time, Unix seconds (inclusive). Defaults to 7 days
        /// before --end-time.
        #[arg(long, value_parser = clap::value_parser!(i64).range(0..))]
        start_time: Option<i64>,
        /// Latest event time, Unix seconds (exclusive). Defaults to now.
        #[arg(long, value_parser = clap::value_parser!(i64).range(0..))]
        end_time: Option<i64>,
        /// Number of items per page (1–100). Use 100 when reading many: the
        /// rate limit counts requests, not events.
        #[arg(long, value_parser = clap::value_parser!(u32).range(1..=100))]
        page_size: Option<u32>,
        /// Continuation token from a previous response.
        #[arg(long)]
        continuation_token: Option<String>,
    },
}

/// Execute an `audit-log` subcommand.
pub fn run(
    command: AuditLogCommand,
    profile: Option<String>,
    base_url: Option<String>,
) -> Result<()> {
    let AuditLogCommand::List {
        category,
        actions,
        actor_ids,
        target_type,
        target_id,
        result,
        start_time,
        end_time,
        page_size,
        continuation_token,
    } = command;
    if let (Some(start), Some(end)) = (start_time, end_time) {
        if start >= end {
            bail!("--start-time must be before --end-time");
        }
        if end - start > MAX_AUDIT_WINDOW_SECS {
            bail!("the window may span at most 92 days");
        }
    }

    let client = api_client(profile, base_url)?;
    let data = list_audit_logs(
        &client,
        &ListAuditLogsParams {
            page_size,
            continuation_token,
            category,
            actions,
            actor_ids,
            target_type,
            target_id,
            result,
            start_time,
            end_time,
        },
    )
    .context("list audit logs")?;
    print_json(&data)
}
