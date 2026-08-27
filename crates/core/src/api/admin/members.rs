//! Members of the team (`/admin/v1/members`).

use serde::Serialize;

use crate::client::Client;
use crate::error::Result;

use super::path;
use super::types::{ListParams, Member, Page};
use super::{EmptyData, idempotency_headers};

/// Request body for creating a virtual member.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct CreateMemberRequest {
    /// Display name of the virtual member.
    pub display_name: String,
    /// Role: `tenant_admin`, `tenant_member`, or a custom role key. The owner
    /// role cannot be assigned.
    pub role: String,
}

/// Request body for changing a member's role.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
struct SetRoleRequest<'a> {
    role: &'a str,
}

/// List the team roster. Any member may read it; contact details come back
/// only to owners and admins, and `name_fuzzy` matches email/username only
/// for them too.
pub fn list_members(client: &Client, params: &ListParams) -> Result<Page<Member>> {
    client.get_data(path::MEMBERS, &params.to_query())
}

/// Create a VIRTUAL member: a login-less managed identity that holds a role
/// and acts only through API keys issued for it (`api-key create
/// --member-principal-id`). Humans join through invitations only.
pub fn create_member(
    client: &Client,
    request: &CreateMemberRequest,
    idempotency_key: Option<&str>,
) -> Result<Member> {
    client.post_data_with_headers(
        path::MEMBERS,
        request,
        &idempotency_headers(idempotency_key),
    )
}

/// Change a member's role. The owner role cannot be assigned, and the owner's
/// own role cannot be changed.
pub fn set_member_role(
    client: &Client,
    principal_id: &str,
    role: &str,
    idempotency_key: Option<&str>,
) -> Result<()> {
    client
        .patch_data_with_headers::<EmptyData, _>(
            &path::member(principal_id),
            &SetRoleRequest { role },
            &idempotency_headers(idempotency_key),
        )
        .map(|_| ())
}

/// Remove a member. Their API keys in this team are disabled (not deleted).
/// The team owner cannot be removed, and the caller cannot remove themselves.
pub fn remove_member(
    client: &Client,
    principal_id: &str,
    idempotency_key: Option<&str>,
) -> Result<()> {
    client
        .delete_data_with_headers::<EmptyData>(
            &path::member(principal_id),
            &idempotency_headers(idempotency_key),
        )
        .map(|_| ())
}
