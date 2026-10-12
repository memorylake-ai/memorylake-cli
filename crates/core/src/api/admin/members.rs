//! Members of the team (`/admin/v1/members`).

use serde::Serialize;

use crate::client::Client;
use crate::error::Result;

use super::path;
use super::types::{Member, Page, push_each, push_opt, push_page_query};
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

/// Largest per-member usage cap the API accepts, in tokens.
pub const MAX_MEMBER_TOKENS: i64 = 1_374_389_534;

/// Request body for changing a member. Only the fields set are sent, and
/// only those change; the server rejects `null`.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize)]
pub struct UpdateMemberRequest {
    /// New role: `tenant_admin`, `tenant_member`, or a custom role key. The
    /// owner role cannot be assigned.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub role: Option<String>,
    /// Per-member usage cap in tokens, at most [`MAX_MEMBER_TOKENS`]; `0`
    /// removes the cap.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub max_tokens: Option<i64>,
}

/// Query parameters for listing members.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct ListMembersParams {
    /// Page size (server default 20, maximum 100).
    pub page_size: Option<u32>,
    /// Continuation token from a previous page.
    pub continuation_token: Option<String>,
    /// Case-insensitive substring filter on the display name; for owners and
    /// admins it also matches email and username.
    pub name_fuzzy: Option<String>,
    /// Only members holding this role key. An unknown key matches nobody.
    pub role: Option<String>,
    /// Only members with these email addresses (exact, case-insensitive),
    /// sent as one repeated `email` parameter each.
    pub emails: Vec<String>,
    /// Only `human` or only `virtual` members.
    pub member_type: Option<String>,
}

/// List the team roster. Any member may read it; contact details come back
/// only to owners and admins, and `name_fuzzy` matches email/username only
/// for them too.
pub fn list_members(client: &Client, params: &ListMembersParams) -> Result<Page<Member>> {
    let mut query = Vec::new();
    push_page_query(
        &mut query,
        params.page_size,
        params.continuation_token.as_deref(),
    );
    push_opt(&mut query, "name_fuzzy", params.name_fuzzy.as_ref());
    push_opt(&mut query, "role", params.role.as_ref());
    push_each(&mut query, "email", &params.emails);
    push_opt(&mut query, "member_type", params.member_type.as_ref());
    client.get_data(path::MEMBERS, &query)
}

/// Return one member, in the roster's shape. A principal outside the team is
/// reported as not found.
pub fn get_member(client: &Client, principal_id: &str) -> Result<Member> {
    client.get_data(&path::member(principal_id), &[])
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

/// Change a member's role and/or usage cap. The owner role cannot be
/// assigned, and neither the owner's role nor the caller's own can be
/// changed.
pub fn update_member(
    client: &Client,
    principal_id: &str,
    request: &UpdateMemberRequest,
    idempotency_key: Option<&str>,
) -> Result<()> {
    client
        .patch_data_with_headers::<EmptyData, _>(
            &path::member(principal_id),
            request,
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
