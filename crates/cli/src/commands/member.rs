//! `memorylake member` commands.

use anyhow::{Context, Result};
use clap::Subcommand;
use memorylake_core::api::admin::{
    CreateMemberRequest, ListParams, create_member, list_members, remove_member, set_member_role,
};

use super::{api_client, print_json};

/// Member subcommands.
#[derive(Debug, Subcommand)]
pub enum MemberCommand {
    /// List the team roster. Contact details appear only for owners/admins.
    List {
        /// Number of items per page.
        #[arg(long)]
        page_size: Option<u32>,
        /// Continuation token from a previous response.
        #[arg(long)]
        continuation_token: Option<String>,
        /// Fuzzy filter by display name (owners/admins also match email and
        /// username).
        #[arg(long = "name")]
        name_fuzzy: Option<String>,
    },
    /// Create a virtual member: a login-less identity that acts only through
    /// API keys issued for it (`api-key create --member`).
    Create {
        /// Display name of the virtual member.
        #[arg(long)]
        name: String,
        /// Role: tenant_admin, tenant_member, or a custom role key.
        #[arg(long)]
        role: String,
        /// Retrying with the same value replays the first result.
        #[arg(long)]
        idempotency_key: Option<String>,
    },
    /// Change a member's role. The owner role cannot be assigned this way.
    SetRole {
        /// Member principal id (from `member list`).
        principal_id: String,
        /// New role: tenant_admin, tenant_member, or a custom role key.
        #[arg(long)]
        role: String,
        /// Retrying with the same value replays the first result.
        #[arg(long)]
        idempotency_key: Option<String>,
    },
    /// Remove a member. Their API keys in this team are disabled, not
    /// deleted. The owner cannot be removed; neither can yourself.
    Remove {
        /// Member principal id (from `member list`).
        principal_id: String,
        /// Retrying with the same value replays the first result.
        #[arg(long)]
        idempotency_key: Option<String>,
    },
}

/// Execute a `member` subcommand.
pub fn run(
    command: MemberCommand,
    profile: Option<String>,
    base_url: Option<String>,
) -> Result<()> {
    let client = api_client(profile, base_url)?;

    match command {
        MemberCommand::List {
            page_size,
            continuation_token,
            name_fuzzy,
        } => {
            let data = list_members(
                &client,
                &ListParams {
                    page_size,
                    continuation_token,
                    name_fuzzy,
                },
            )
            .context("list members")?;
            print_json(&data)
        }
        MemberCommand::Create {
            name,
            role,
            idempotency_key,
        } => {
            let data = create_member(
                &client,
                &CreateMemberRequest {
                    display_name: name,
                    role,
                },
                idempotency_key.as_deref(),
            )
            .context("create virtual member")?;
            print_json(&data)
        }
        MemberCommand::SetRole {
            principal_id,
            role,
            idempotency_key,
        } => {
            set_member_role(&client, &principal_id, &role, idempotency_key.as_deref())
                .context("change member role")?;
            println!("member {principal_id} now has role {role}");
            Ok(())
        }
        MemberCommand::Remove {
            principal_id,
            idempotency_key,
        } => {
            remove_member(&client, &principal_id, idempotency_key.as_deref())
                .context("remove member")?;
            println!("member {principal_id} removed");
            Ok(())
        }
    }
}
