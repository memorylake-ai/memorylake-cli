//! The team itself (`/admin/v1/team`), and creating new ones
//! (`/admin/v1/teams`).

use serde::Serialize;

use crate::client::Client;
use crate::error::Result;

use super::idempotency_headers;
use super::path;
use super::types::{CreatedTeam, DeletedTeam, OwnershipTransfer, Team};

/// Request body for renaming the team.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
struct RenameTeamRequest<'a> {
    name: &'a str,
}

/// Request body for creating a team.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize)]
pub struct CreateTeamRequest {
    /// Display name of the new org team.
    pub name: String,
    /// Also create an owner API key in the new team and return it once.
    #[serde(skip_serializing_if = "std::ops::Not::not")]
    pub issue_api_key: bool,
    /// Display name of that key. The server defaults it to the team name
    /// plus " API key".
    #[serde(skip_serializing_if = "Option::is_none")]
    pub api_key_name: Option<String>,
}

/// Request body for handing the team to a new owner.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
struct TransferOwnershipRequest<'a> {
    new_owner_principal_id: &'a str,
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

/// Create a new org team owned by the key's owner. It counts toward the
/// owner's team limit. The calling key stays in its own team; only the key
/// returned here (when asked for) addresses the new one.
pub fn create_team(
    client: &Client,
    request: &CreateTeamRequest,
    idempotency_key: Option<&str>,
) -> Result<CreatedTeam> {
    client.post_data_with_headers(path::TEAMS, request, &idempotency_headers(idempotency_key))
}

/// Permanently dissolve the key's org team. Every team API key, the calling
/// one included, stops working at once, and a paid subscription is cancelled
/// first. Personal teams cannot be deleted.
///
/// The endpoint documents no `Idempotency-Key`.
pub fn delete_team(client: &Client) -> Result<DeletedTeam> {
    client.delete_data(path::TEAM)
}

/// Hand the org team to another active human member. The caller becomes an
/// admin and loses every owner-only power.
pub fn transfer_team_ownership(
    client: &Client,
    new_owner_principal_id: &str,
    idempotency_key: Option<&str>,
) -> Result<OwnershipTransfer> {
    client.post_data_with_headers(
        path::TEAM_TRANSFER_OWNERSHIP,
        &TransferOwnershipRequest {
            new_owner_principal_id,
        },
        &idempotency_headers(idempotency_key),
    )
}
