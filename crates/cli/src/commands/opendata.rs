//! `memorylake opendata` commands.

use anyhow::{Context, Result};
use clap::Subcommand;
use memorylake_core::api::opendata::list_opendata;

use super::{api_client, print_json};

/// Opendata subcommands.
#[derive(Debug, Subcommand)]
pub enum OpendataCommand {
    /// List the opendata collections a project can draw on. The `id` is what
    /// `project create|update --opendata-ids` accepts.
    List,
}

/// Execute an `opendata` subcommand.
pub fn run(
    command: OpendataCommand,
    profile: Option<String>,
    base_url: Option<String>,
) -> Result<()> {
    let client = api_client(profile, base_url)?;

    match command {
        OpendataCommand::List => {
            let data = list_opendata(&client).context("list opendata")?;
            print_json(&data)
        }
    }
}
