//! `memorylake industry` commands.

use anyhow::{Context, Result};
use clap::Subcommand;
use memorylake_core::api::industries::list_industries;

use super::{api_client, print_json};

/// Industry subcommands.
#[derive(Debug, Subcommand)]
pub enum IndustryCommand {
    /// List the industry opendata collections a project can draw on. The `id`
    /// is what `project create|update --industry-ids` accepts.
    List,
}

/// Execute an `industry` subcommand.
pub fn run(
    command: IndustryCommand,
    profile: Option<String>,
    base_url: Option<String>,
) -> Result<()> {
    let client = api_client(profile, base_url)?;

    match command {
        IndustryCommand::List => {
            let data = list_industries(&client).context("list industry opendata")?;
            print_json(&data)
        }
    }
}
