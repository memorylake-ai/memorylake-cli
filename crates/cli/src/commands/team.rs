//! `memorylake team` commands.

use anyhow::{Context, Result};
use clap::Subcommand;
use memorylake_core::api::admin::{get_team, rename_team};

use super::{api_client, print_json};

/// Team subcommands.
#[derive(Debug, Subcommand)]
pub enum TeamCommand {
    /// Show the team this API key belongs to.
    Get,
    /// Rename the team. Only the team owner may do this.
    Rename {
        /// New display name.
        #[arg(long)]
        name: String,
        /// Retrying with the same value replays the first result.
        #[arg(long)]
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
    }
}
