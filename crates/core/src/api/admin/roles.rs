//! Roles of the team (`/admin/v1/roles`).

use serde::{Deserialize, Serialize};

use crate::client::Client;
use crate::error::Result;

use super::path;

/// The team's role catalog: built-in roles first (owner, admin, member),
/// then this team's custom roles oldest-first.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RoleList {
    /// Every role of the team.
    #[serde(default = "Vec::new")]
    pub roles: Vec<Role>,
}

/// One role. Its `key` is what the `--role` flags of `member` and
/// `invitation` commands accept.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Role {
    /// The value the role fields of member and invitation writes accept.
    pub key: String,
    /// Display name: fixed English for built-ins, operator-authored for
    /// custom roles.
    pub label: String,
    /// Operator-authored description. Only custom roles carry one.
    #[serde(default)]
    pub description: Option<String>,
    /// true for the three roles every team has.
    pub built_in: bool,
    /// Whether member and invitation writes accept this role. The owner role
    /// never is — ownership transfer stays in the console.
    pub assignable: bool,
    /// When true, only a caller whose own role is owner or admin may grant it.
    pub admin_grant_only: bool,
    /// For custom roles: the role its policies were copied from at creation.
    #[serde(default)]
    pub parent_role_key: Option<String>,
}

/// List the team's roles. Any member may read this.
pub fn list_roles(client: &Client) -> Result<RoleList> {
    client.get_data(path::ROLES, &[])
}
