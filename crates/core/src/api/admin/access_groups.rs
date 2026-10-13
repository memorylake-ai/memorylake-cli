//! Access groups (`/admin/v1/access-groups`).
//!
//! An access group gives its members extra permissions and access to extra
//! instances; it never takes access away. Its permissions come from an
//! optional base (a role or another group, fixed at creation) plus its own
//! list.

use serde::{Deserialize, Serialize};
use serde_json::{Map, Value};

use crate::client::Client;
use crate::error::Result;

use super::path;
use super::scopes::{AccessGroupScope, SetScopesRequest};
use super::types::{Page, push_page_query};
use super::{EmptyData, idempotency_headers, write_headers};

/// One permission: an action on a resource type.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
pub struct AccessGroupPermission {
    /// Resource type, e.g. `memorylake:actor`.
    pub resource_type: String,
    /// Action, e.g. `actor:create`.
    pub action: String,
}

impl AccessGroupPermission {
    /// Parse `<resource_type>:<action>` — the `permissions` notation of
    /// `permission me` and role details, e.g. `memorylake:actor:actor:create`.
    /// A resource type is two `:`-separated words and an action at least two
    /// more, so the split falls after the second colon.
    pub fn parse(raw: &str) -> std::result::Result<Self, String> {
        let raw = raw.trim();
        let mut parts = raw.splitn(3, ':');
        let (Some(prefix), Some(kind), Some(action)) = (parts.next(), parts.next(), parts.next())
        else {
            return Err(format!(
                "`{raw}` is not <resource_type>:<action>, e.g. memorylake:actor:actor:create"
            ));
        };
        if prefix.is_empty()
            || kind.is_empty()
            || !action.contains(':')
            || action.starts_with(':')
            || action.ends_with(':')
        {
            return Err(format!(
                "`{raw}` is not <resource_type>:<action>, e.g. memorylake:actor:actor:create"
            ));
        }
        Ok(Self {
            resource_type: format!("{prefix}:{kind}"),
            action: action.to_string(),
        })
    }
}

/// An access group.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AccessGroup {
    /// Identifier.
    pub group_id: String,
    /// Display name.
    pub name: String,
    /// Description.
    #[serde(default)]
    pub description: Option<String>,
    /// The group grants everything this role grants. Fixed at creation.
    #[serde(default)]
    pub base_role_key: Option<String>,
    /// The group grants everything this other group grants. Fixed at
    /// creation.
    #[serde(default)]
    pub base_access_group_id: Option<String>,
    /// Permissions on top of the base.
    #[serde(default)]
    pub permissions: Vec<AccessGroupPermission>,
    /// Number of members.
    pub member_count: i64,
    /// Instances the group gives access to, counted per resource type.
    #[serde(default)]
    pub scope_summary: Option<Vec<Value>>,
    /// `principal_id` of the creator.
    pub created_by: String,
    /// Unix seconds.
    pub created_at: i64,
    /// Unix seconds.
    pub updated_at: i64,
    /// Fields the CLI does not model yet.
    #[serde(flatten)]
    pub extra: Map<String, Value>,
}

/// A member of an access group.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AccessGroupMember {
    /// The member's `principal_id`.
    pub principal_id: String,
    /// Display name.
    pub display_name: String,
    /// Fields the CLI does not model yet.
    #[serde(flatten)]
    pub extra: Map<String, Value>,
}

/// The group's own permission list after a replacement.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AccessGroupPermissions {
    /// The group.
    pub group_id: String,
    /// Permissions on top of its base.
    #[serde(default)]
    pub permissions: Vec<AccessGroupPermission>,
    /// Fields the CLI does not model yet.
    #[serde(flatten)]
    pub extra: Map<String, Value>,
}

/// Outcome of adding members.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AddedAccessGroupMembers {
    /// How many were added; members already in the group count too.
    pub added: i64,
    /// Fields the CLI does not model yet.
    #[serde(flatten)]
    pub extra: Map<String, Value>,
}

/// Everything a group grants, each permission with where it comes from.
///
/// The nested breakdowns are printed as the server sends them: the CLI only
/// reports them, and their source records vary by kind.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AccessGroupExplanation {
    /// The group.
    pub group_id: String,
    /// Display name.
    pub name: String,
    /// Number of members.
    pub member_count: i64,
    /// The base role or base group, when there is one.
    #[serde(default)]
    pub inherits_from: Option<Value>,
    /// Every permission the group grants, with its source.
    #[serde(default)]
    pub permissions: Vec<Value>,
    /// Permissions in the group's own list that do not take effect.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub overridden_permissions: Option<Vec<Value>>,
    /// Instances the group gives access to, per resource type.
    #[serde(default)]
    pub scopes: Vec<Value>,
    /// Fields the CLI does not model yet.
    #[serde(flatten)]
    pub extra: Map<String, Value>,
}

/// Request body for creating an access group.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize)]
pub struct CreateAccessGroupRequest {
    /// Display name.
    pub name: String,
    /// Description.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    /// Base role: `tenant_member` or `tenant_admin` (the latter needs an
    /// owner or admin). Exclusive with `base_access_group_id`.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub base_role_key: Option<String>,
    /// Base access group. Exclusive with `base_role_key`.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub base_access_group_id: Option<String>,
    /// Permissions on top of the base.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub permissions: Option<Vec<AccessGroupPermission>>,
}

/// Request body for renaming or re-describing an access group.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize)]
pub struct UpdateAccessGroupRequest {
    /// New display name.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,
    /// New description; `""` clears it.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
}

#[derive(Serialize)]
struct AddMembersRequest<'a> {
    principal_ids: &'a [String],
}

#[derive(Serialize)]
struct SetPermissionsRequest<'a> {
    permissions: &'a [AccessGroupPermission],
    reason: &'a str,
}

/// List the team's access groups.
pub fn list_access_groups(
    client: &Client,
    page_size: Option<u32>,
    continuation_token: Option<&str>,
) -> Result<Page<AccessGroup>> {
    let mut query = Vec::new();
    push_page_query(&mut query, page_size, continuation_token);
    client.get_data(path::ACCESS_GROUPS, &query)
}

/// Return one access group and the `ETag` of its permission list, which
/// [`set_access_group_permissions`] takes back as `If-Match`.
pub fn get_access_group(client: &Client, id: &str) -> Result<(AccessGroup, Option<String>)> {
    client.get_data_with_etag(&path::access_group(id), &[])
}

/// Create an access group. A team has at most 200.
pub fn create_access_group(
    client: &Client,
    request: &CreateAccessGroupRequest,
    idempotency_key: Option<&str>,
) -> Result<AccessGroup> {
    client.post_data_with_headers(
        path::ACCESS_GROUPS,
        request,
        &idempotency_headers(idempotency_key),
    )
}

/// Rename an access group or change its description. The base cannot
/// change.
pub fn update_access_group(
    client: &Client,
    id: &str,
    request: &UpdateAccessGroupRequest,
    idempotency_key: Option<&str>,
) -> Result<AccessGroup> {
    client.patch_data_with_headers(
        &path::access_group(id),
        request,
        &idempotency_headers(idempotency_key),
    )
}

/// Delete an access group; its members lose what it gave them. Refused
/// (409) while other groups are based on it.
pub fn delete_access_group(client: &Client, id: &str, idempotency_key: Option<&str>) -> Result<()> {
    client
        .delete_data_with_headers::<EmptyData>(
            &path::access_group(id),
            &idempotency_headers(idempotency_key),
        )
        .map(|_| ())
}

/// List a group's members.
pub fn list_access_group_members(
    client: &Client,
    id: &str,
    page_size: Option<u32>,
    continuation_token: Option<&str>,
) -> Result<Page<AccessGroupMember>> {
    let mut query = Vec::new();
    push_page_query(&mut query, page_size, continuation_token);
    client.get_data(&path::access_group_members(id), &query)
}

/// Add up to 200 members, all or nothing: one id outside the team adds
/// nobody (404). Members already in the group are not an error.
pub fn add_access_group_members(
    client: &Client,
    id: &str,
    principal_ids: &[String],
    idempotency_key: Option<&str>,
) -> Result<AddedAccessGroupMembers> {
    client.post_data_with_headers(
        &path::access_group_members(id),
        &AddMembersRequest { principal_ids },
        &idempotency_headers(idempotency_key),
    )
}

/// Remove one member from a group (404 when not in it).
pub fn remove_access_group_member(
    client: &Client,
    id: &str,
    principal_id: &str,
    idempotency_key: Option<&str>,
) -> Result<()> {
    client
        .delete_data_with_headers::<EmptyData>(
            &path::access_group_member(id, principal_id),
            &idempotency_headers(idempotency_key),
        )
        .map(|_| ())
}

/// Replace the group's own permission list; an empty list removes them all.
/// Only permissions the catalog lists as grantable to access groups are
/// accepted.
pub fn set_access_group_permissions(
    client: &Client,
    id: &str,
    permissions: &[AccessGroupPermission],
    reason: &str,
    if_match: Option<&str>,
) -> Result<AccessGroupPermissions> {
    client.put_data_with_headers(
        &path::access_group_permissions(id),
        &SetPermissionsRequest {
            permissions,
            reason,
        },
        &write_headers(None, if_match),
    )
}

/// Return the instances the group gives access to, and the `ETag`
/// [`set_access_group_scope`] takes as `If-Match`.
pub fn get_access_group_scope(
    client: &Client,
    id: &str,
) -> Result<(AccessGroupScope, Option<String>)> {
    client.get_data_with_etag(&path::access_group_scope(id), &[])
}

/// Replace, per listed resource type, the instances the group gives access
/// to. Mode `all` removes the type's instances: a group never limits anyone.
pub fn set_access_group_scope(
    client: &Client,
    id: &str,
    request: &SetScopesRequest,
    if_match: Option<&str>,
) -> Result<AccessGroupScope> {
    client.put_data_with_headers(
        &path::access_group_scope(id),
        request,
        &write_headers(None, if_match),
    )
}

/// Explain everything the group grants and where each permission comes from.
pub fn explain_access_group(client: &Client, id: &str) -> Result<AccessGroupExplanation> {
    client.get_data(&path::access_group_effective(id), &[])
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn permissions_split_after_the_resource_type() {
        let parsed =
            AccessGroupPermission::parse("memorylake:workspace_actor:workspace_actor:read")
                .unwrap();
        assert_eq!(parsed.resource_type, "memorylake:workspace_actor");
        assert_eq!(parsed.action, "workspace_actor:read");
    }

    #[test]
    fn malformed_permissions_are_refused() {
        for raw in [
            "",
            "memorylake",
            "memorylake:actor",
            "memorylake:actor:create",
            "a:b:c:",
            ":b:c:d",
        ] {
            assert!(AccessGroupPermission::parse(raw).is_err(), "{raw:?}");
        }
    }
}
