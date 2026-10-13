//! `memorylake audit-log` commands.

use anyhow::{Context, Result, bail};
use clap::Subcommand;
use memorylake_core::api::admin::{
    ListAuditLogsParams, MAX_AUDIT_ACTIONS, MAX_AUDIT_ACTORS, MAX_AUDIT_TARGET_ID,
    MAX_AUDIT_TARGET_TYPE, MAX_AUDIT_WINDOW_SECS, list_audit_logs,
};

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
        /// Only this action, e.g. member:change_role, api_key:create or
        /// billing:set_balance_alert (the `action` of earlier events). Repeat,
        /// or separate with commas, for several (at most 50).
        #[arg(long = "action", value_delimiter = ',', value_parser = parse_non_blank)]
        actions: Vec<String>,
        /// Only events performed by this member (principal id, from `member
        /// list`). Repeat, or separate with commas, for several (at most
        /// 100). Also accepted as --member, as in `usage records`.
        #[arg(long = "actor", visible_alias = "member", value_name = "PRINCIPAL_ID", value_delimiter = ',', value_parser = parse_non_blank)]
        actor_ids: Vec<String>,
        /// Only events on this kind of target, e.g. principal (at most 32
        /// characters).
        #[arg(long, value_parser = parse_non_blank)]
        target_type: Option<String>,
        /// Only events on this target id (at most 128 characters).
        #[arg(long, value_parser = parse_non_blank)]
        target_id: Option<String>,
        /// Only events with this outcome.
        #[arg(long, value_parser = ["succeeded", "failed", "partial"])]
        result: Option<String>,
        /// Earliest event time, Unix seconds (inclusive). Defaults to 7 days
        /// before --end-time.
        #[arg(long, value_parser = clap::value_parser!(i64).range(1..))]
        start_time: Option<i64>,
        /// Latest event time, Unix seconds (exclusive). Defaults to now.
        #[arg(long, value_parser = clap::value_parser!(i64).range(1..))]
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
            bail!("--start-time ({start}) must be before --end-time ({end})");
        }
        if end - start > MAX_AUDIT_WINDOW_SECS {
            bail!(
                "--start-time and --end-time are more than 92 days apart; \
                 list the events in several windows"
            );
        }
    }
    if actions.len() > MAX_AUDIT_ACTIONS {
        bail!(
            "at most {MAX_AUDIT_ACTIONS} --action values, got {}",
            actions.len()
        );
    }
    if actor_ids.len() > MAX_AUDIT_ACTORS {
        bail!(
            "at most {MAX_AUDIT_ACTORS} --actor values, got {}",
            actor_ids.len()
        );
    }
    for (flag, value, max) in [
        ("--target-type", &target_type, MAX_AUDIT_TARGET_TYPE),
        ("--target-id", &target_id, MAX_AUDIT_TARGET_ID),
    ] {
        if let Some(value) = value
            && value.chars().count() > max
        {
            bail!("{flag} is at most {max} characters");
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
