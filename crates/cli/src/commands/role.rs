//! `memorylake role` commands.

use anyhow::{Context, Result};
use clap::Subcommand;
use memorylake_core::api::admin::list_roles;

use super::{api_client, print_json};

/// Role subcommands.
#[derive(Debug, Subcommand)]
pub enum RoleCommand {
    /// List the team's roles: built-ins first, then custom roles. The `key`
    /// is what `member` and `invitation` commands accept as `--role`.
    List,
}

/// Execute a `role` subcommand.
pub fn run(command: RoleCommand, profile: Option<String>, base_url: Option<String>) -> Result<()> {
    let client = api_client(profile, base_url)?;

    match command {
        RoleCommand::List => {
            let data = list_roles(&client).context("list roles")?;
            print_json(&data)
        }
    }
}
