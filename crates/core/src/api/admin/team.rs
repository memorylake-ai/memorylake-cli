//! The team itself (`/admin/v1/team`).

use serde::Serialize;

use crate::client::Client;
use crate::error::Result;

use super::idempotency_headers;
use super::path;
use super::types::Team;

/// Request body for renaming the team.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
struct RenameTeamRequest<'a> {
    name: &'a str,
}

/// Return the team the key belongs to. Any member may read it.
pub fn get_team(client: &Client) -> Result<Team> {
    client.get_data(path::TEAM, &[])
}

/// Rename the team. Only the team owner may do this.
pub fn rename_team(client: &Client, name: &str, idempotency_key: Option<&str>) -> Result<Team> {
    client.patch_data_with_headers(
        path::TEAM,
        &RenameTeamRequest { name },
        &idempotency_headers(idempotency_key),
    )
}
