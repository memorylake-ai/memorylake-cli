//! Invitations to the team (`/admin/v1/invitations`).

use serde::Serialize;

use crate::client::Client;
use crate::error::Result;

use super::path;
use super::types::{Invitation, Page, push_page_query};
use super::{EmptyData, idempotency_headers};

/// Request body for inviting someone to the team.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct CreateInvitationRequest {
    /// Invitee email address. One live invitation per address per team.
    pub email: String,
    /// Role the invitee will hold on acceptance: `tenant_admin`,
    /// `tenant_member`, or a custom role key. The owner role cannot be
    /// invited.
    pub role: String,
}

/// Query parameters for listing invitations.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct ListInvitationsParams {
    /// Page size (server default 20, maximum 100).
    pub page_size: Option<u32>,
    /// Continuation token from a previous page.
    pub continuation_token: Option<String>,
    /// Only return invitations in this state: `pending`, `accepted`,
    /// `rejected`, `expired`, or `revoked`.
    pub status: Option<String>,
}

/// Invite someone by email. Re-inviting is revoke + create; there is no
/// resend endpoint. Invitation emails expire after 7 days.
pub fn create_invitation(
    client: &Client,
    request: &CreateInvitationRequest,
    idempotency_key: Option<&str>,
) -> Result<Invitation> {
    client.post_data_with_headers(
        path::INVITATIONS,
        request,
        &idempotency_headers(idempotency_key),
    )
}

/// List the team's invitations, newest first.
pub fn list_invitations(
    client: &Client,
    params: &ListInvitationsParams,
) -> Result<Page<Invitation>> {
    let mut query = Vec::new();
    push_page_query(
        &mut query,
        params.page_size,
        params.continuation_token.as_deref(),
    );
    if let Some(status) = &params.status {
        query.push(("status", status.clone()));
    }
    client.get_data(path::INVITATIONS, &query)
}

/// Revoke a pending invitation; its email link stops working. Accepted
/// invitations cannot be revoked — remove the member instead.
pub fn revoke_invitation(client: &Client, id: &str, idempotency_key: Option<&str>) -> Result<()> {
    client
        .delete_data_with_headers::<EmptyData>(
            &path::invitation(id),
            &idempotency_headers(idempotency_key),
        )
        .map(|_| ())
}
