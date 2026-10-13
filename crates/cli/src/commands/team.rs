//! `memorylake team` commands.

use anyhow::{Context, Result, bail};
use clap::Subcommand;
use memorylake_core::Client;
use memorylake_core::api::admin::{
    CreateTeamRequest, Team, create_team, delete_team, get_team, rename_team,
    transfer_team_ownership,
};

use super::api_key::warn_if_key_withheld;
use super::{api_client, parse_non_blank, print_json};

/// Team subcommands.
#[derive(Debug, Subcommand)]
pub enum TeamCommand {
    /// Show the team this API key belongs to.
    Get,
    /// Rename the team. Only the team owner may do this.
    #[command(visible_alias = "update")]
    Rename {
        /// New display name.
        #[arg(long, value_parser = parse_non_blank)]
        name: String,
        /// Retrying with the same value replays the first result.
        #[arg(long, value_parser = parse_non_blank)]
        idempotency_key: Option<String>,
    },
    /// Create a new org team owned by you. It counts toward your team limit.
    ///
    /// The current key stays in its own team. To act in the new one, ask for
    /// an owner key with --issue-api-key: it is printed exactly once.
    Create {
        /// Display name of the new team.
        #[arg(long, value_parser = parse_non_blank)]
        name: String,
        /// Also create an owner API key in the new team and print it once.
        #[arg(long)]
        issue_api_key: bool,
        /// Display name of that key. Defaults to the team name plus
        /// " API key".
        #[arg(long, requires = "issue_api_key", value_parser = parse_non_blank)]
        api_key_name: Option<String>,
        /// Retrying with the same value replays the first result instead of
        /// creating a second team.
        #[arg(long, value_parser = parse_non_blank)]
        idempotency_key: Option<String>,
    },
    /// PERMANENTLY delete this org team. Owner only; irreversible.
    ///
    /// Every API key of the team — including the one running this command —
    /// stops working at once, and a paid subscription is cancelled first.
    /// Personal teams cannot be deleted.
    ///
    /// --confirm must repeat the id `team get` reports; the command reads
    /// the team first and refuses on a mismatch.
    Delete {
        /// The id of the team being deleted, as `team get` reports it.
        #[arg(long, value_name = "TEAM_ID", value_parser = parse_non_blank)]
        confirm: String,
    },
    /// Hand this org team to another member. Owner only.
    ///
    /// The new owner must be an active human member. You become an admin and
    /// lose every owner-only power, including getting it back yourself.
    ///
    /// --confirm must repeat the id `team get` reports; the command reads
    /// the team first and refuses on a mismatch.
    TransferOwnership {
        /// Principal id of the new owner (from `member list`).
        #[arg(long = "to", value_name = "PRINCIPAL_ID", value_parser = parse_non_blank)]
        new_owner_principal_id: String,
        /// The id of this team, as `team get` reports it.
        #[arg(long, value_name = "TEAM_ID", value_parser = parse_non_blank)]
        confirm: String,
        /// Retrying with the same value replays the first result.
        #[arg(long, value_parser = parse_non_blank)]
        idempotency_key: Option<String>,
    },
}

/// Execute a `team` subcommand.
pub fn run(command: TeamCommand, profile: Option<String>, base_url: Option<String>) -> Result<()> {
    let client = api_client(profile, base_url)?;

    match command {
        TeamCommand::Get => {
            let data = get_team(&client).context("get the team")?;
            print_json(&data)
        }
        TeamCommand::Rename {
            name,
            idempotency_key,
        } => {
            let data = rename_team(&client, &name, idempotency_key.as_deref())
                .context("rename the team")?;
            print_json(&data)
        }
        TeamCommand::Create {
            name,
            issue_api_key,
            api_key_name,
            idempotency_key,
        } => {
            let data = create_team(
                &client,
                &CreateTeamRequest {
                    name,
                    issue_api_key,
                    api_key_name,
                },
                idempotency_key.as_deref(),
            )
            .context("create a team")?;
            print_json(&data)?;
            if issue_api_key {
                warn_if_key_withheld(
                    data.api_key.as_ref().and_then(|key| key.key.as_deref()),
                    "create a key for the new team in the MemoryLake console",
                );
            }
            Ok(())
        }
        TeamCommand::Delete { confirm } => {
            check_confirmation(&client, &confirm, "delete")?;
            let data = delete_team(&client).context("delete the team")?;
            print_json(&data)?;
            eprintln!(
                "team {confirm} deleted; this profile's API key no longer works — \
                 log in again with a key of another team"
            );
            Ok(())
        }
        TeamCommand::TransferOwnership {
            new_owner_principal_id,
            confirm,
            idempotency_key,
        } => {
            check_confirmation(&client, &confirm, "transfer")?;
            let data = transfer_team_ownership(
                &client,
                &new_owner_principal_id,
                idempotency_key.as_deref(),
            )
            .context("transfer team ownership")?;
            print_json(&data)
        }
    }
}

/// Refuse a team-wide irreversible action unless `confirm` names the team
/// the key actually belongs to, and the team is one the action applies to.
///
/// The key decides which team is affected — no argument can — so a copied
/// command run under the wrong profile would otherwise hit whichever team
/// that profile's key happens to belong to.
fn check_confirmation(client: &Client, confirm: &str, action: &str) -> Result<()> {
    let team = get_team(client).context("read the team to confirm against")?;
    check_team(&team, confirm, action)
}

/// The local half of [`check_confirmation`].
fn check_team(team: &Team, confirm: &str, action: &str) -> Result<()> {
    if team.id != confirm {
        bail!(
            "refusing to {action}: --confirm {confirm} does not match this key's team {} ({:?})",
            team.id,
            team.name
        );
    }
    if team.team_type != "org" {
        bail!(
            "refusing to {action}: team {} is a {} team; only org teams can be {}",
            team.id,
            team.team_type,
            if action == "delete" {
                "deleted"
            } else {
                "transferred"
            }
        );
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn team(id: &str, team_type: &str) -> Team {
        Team {
            id: id.into(),
            name: "Acme".into(),
            team_type: team_type.into(),
            owner_principal_id: "prin-1".into(),
            created_at: 0,
            caller_role: None,
            extra: Default::default(),
        }
    }

    #[test]
    fn a_matching_org_team_passes() {
        assert!(check_team(&team("t-1", "org"), "t-1", "delete").is_ok());
    }

    #[test]
    fn a_mismatched_id_is_refused() {
        let err = check_team(&team("t-1", "org"), "t-2", "delete").unwrap_err();
        assert!(err.to_string().contains("does not match"), "{err}");
    }

    #[test]
    fn a_personal_team_is_refused() {
        let err = check_team(&team("t-1", "personal"), "t-1", "transfer").unwrap_err();
        assert!(err.to_string().contains("only org teams"), "{err}");
    }
}
