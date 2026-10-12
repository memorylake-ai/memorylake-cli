//! Roles of the team (`/admin/v1/roles`): the built-ins and custom roles.

use serde::{Deserialize, Serialize};
use serde_json::{Map, Value};

use crate::client::Client;
use crate::error::Result;

use super::path;
use super::scopes::{RoleAccessScope, SetScopesRequest};
use super::{EmptyData, idempotency_headers, write_headers};

/// The team's role catalog: built-in roles first (owner, admin, member),
/// then this team's custom roles oldest-first.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RoleList {
    /// Every role of the team.
    #[serde(default = "Vec::new")]
    pub roles: Vec<Role>,
    /// Fields the CLI does not model yet, kept so printing this never drops
    /// what the server sent.
    #[serde(flatten)]
    pub extra: Map<String, Value>,
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
    /// never is — the owner changes only through an ownership transfer.
    pub assignable: bool,
    /// When true, only a caller whose own role is owner or admin may grant it.
    pub admin_grant_only: bool,
    /// For custom roles: the role whose permission rules were copied into
    /// this one at creation. Absent on built-ins and on custom roles created
    /// without a base.
    #[serde(default)]
    pub base_role_key: Option<String>,
    /// Same value as `base_role_key`; the server keeps it for older clients.
    #[serde(default)]
    pub parent_role_key: Option<String>,
    /// Fields the CLI does not model yet, kept so printing this never drops
    /// what the server sent.
    #[serde(flatten)]
    pub extra: Map<String, Value>,
}

/// List the team's roles. Any member may read this.
pub fn list_roles(client: &Client) -> Result<RoleList> {
    client.get_data(path::ROLES, &[])
}

/// One role with every permission it grants.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RoleDetail {
    /// The role itself.
    #[serde(flatten)]
    pub role: Role,
    /// Every permission the role grants and where it comes from. Returned
    /// only to callers holding `policy:manage`; absent for everyone else.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub permissions: Option<Vec<RolePermission>>,
}

/// One permission of a [`RoleDetail`].
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RolePermission {
    /// `<resource_type>:<action>`.
    pub permission: String,
    /// `built_in`, `base_role` (copied from the base at creation), or
    /// `role_rule` (this role's own rules).
    pub source: String,
    /// Fields the CLI does not model yet.
    #[serde(flatten)]
    pub extra: Map<String, Value>,
}

/// Request body for creating a custom role.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize)]
pub struct CreateRoleRequest {
    /// Display name.
    pub label: String,
    /// Description.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    /// Start with a copy of this role's permission rules: `tenant_admin`,
    /// `tenant_member`, or a custom role key. The copy is independent.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub base_role_key: Option<String>,
}

/// Request body for renaming or re-describing a custom role.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize)]
pub struct UpdateRoleRequest {
    /// New display name.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub label: Option<String>,
    /// New description; `""` clears it.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
}

/// Return one role, built-in or custom. Any member may read it.
pub fn get_role(client: &Client, key: &str) -> Result<RoleDetail> {
    client.get_data(&path::role(key), &[])
}

/// Create a custom role. Only the team owner may manage roles.
pub fn create_role(
    client: &Client,
    request: &CreateRoleRequest,
    idempotency_key: Option<&str>,
) -> Result<RoleDetail> {
    client.post_data_with_headers(path::ROLES, request, &idempotency_headers(idempotency_key))
}

/// Rename a custom role or change its description. Built-ins cannot change
/// (409); what a role grants changes through permission rules.
pub fn update_role(
    client: &Client,
    key: &str,
    request: &UpdateRoleRequest,
    idempotency_key: Option<&str>,
) -> Result<RoleDetail> {
    client.patch_data_with_headers(
        &path::role(key),
        request,
        &idempotency_headers(idempotency_key),
    )
}

/// Delete a custom role with its rules and scope. Refused (409) while any
/// member holds it, and for built-ins.
pub fn delete_role(client: &Client, key: &str, idempotency_key: Option<&str>) -> Result<()> {
    client
        .delete_data_with_headers::<EmptyData>(
            &path::role(key),
            &idempotency_headers(idempotency_key),
        )
        .map(|_| ())
}

/// Return the role's access scope and the `ETag` [`set_role_scope`] takes as
/// `If-Match`. The owner role has none (409).
pub fn get_role_scope(client: &Client, key: &str) -> Result<(RoleAccessScope, Option<String>)> {
    client.get_data_with_etag(&path::role_scope(key), &[])
}

/// Replace the scope of each listed resource type for everyone holding the
/// role. A scope limits; it never grants more than the role allows.
pub fn set_role_scope(
    client: &Client,
    key: &str,
    request: &SetScopesRequest,
    if_match: Option<&str>,
) -> Result<RoleAccessScope> {
    client.put_data_with_headers(
        &path::role_scope(key),
        request,
        &write_headers(None, if_match),
    )
}
