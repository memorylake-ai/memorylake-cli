//! `memorylake invitation` / `invite` commands.

use anyhow::{Context, Result};
use clap::Subcommand;
use memorylake_core::api::admin::{
    CreateInvitationRequest, ListInvitationsParams, create_invitation, list_invitations,
    revoke_invitation,
};

use super::{api_client, print_json};

/// Invitation subcommands.
#[derive(Debug, Subcommand)]
pub enum InvitationCommand {
    /// Invite someone to the team by email. One live invitation per address;
    /// re-inviting is revoke + create.
    Create {
        /// Invitee email address.
        #[arg(long)]
        email: String,
        /// Role on acceptance: tenant_admin, tenant_member, or a custom role
        /// key.
        #[arg(long)]
        role: String,
        /// Retrying with the same value replays the first result instead of
        /// sending a second email.
        #[arg(long)]
        idempotency_key: Option<String>,
    },
    /// List the team's invitations, newest first.
    List {
        /// Number of items per page.
        #[arg(long)]
        page_size: Option<u32>,
        /// Continuation token from a previous response.
        #[arg(long)]
        continuation_token: Option<String>,
        /// Only this state: pending, accepted, rejected, expired, revoked.
        #[arg(long)]
        status: Option<String>,
    },
    /// Revoke a pending invitation; its email link stops working.
    Revoke {
        /// Invitation id.
        id: String,
        /// Retrying with the same value replays the first result.
        #[arg(long)]
        idempotency_key: Option<String>,
    },
}

/// Execute an `invitation` subcommand.
pub fn run(
    command: InvitationCommand,
    profile: Option<String>,
    base_url: Option<String>,
) -> Result<()> {
    let client = api_client(profile, base_url)?;

    match command {
        InvitationCommand::Create {
            email,
            role,
            idempotency_key,
        } => {
            let data = create_invitation(
                &client,
                &CreateInvitationRequest { email, role },
                idempotency_key.as_deref(),
            )
            .context("create invitation")?;
            print_json(&data)
        }
        InvitationCommand::List {
            page_size,
            continuation_token,
            status,
        } => {
            let data = list_invitations(
                &client,
                &ListInvitationsParams {
                    page_size,
                    continuation_token,
                    status,
                },
            )
            .context("list invitations")?;
            print_json(&data)
        }
        InvitationCommand::Revoke {
            id,
            idempotency_key,
        } => {
            revoke_invitation(&client, &id, idempotency_key.as_deref())
                .context("revoke invitation")?;
            println!("invitation {id} revoked");
            Ok(())
        }
    }
}
