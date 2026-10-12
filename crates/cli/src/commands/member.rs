//! `memorylake member` commands.

use anyhow::{Context, Result, bail};
use clap::Subcommand;
use memorylake_core::api::admin::{
    CreateMemberRequest, ListMembersParams, MAX_MEMBER_TOKENS, UpdateMemberRequest, create_member,
    get_member, list_members, remove_member, update_member,
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
        /// Only members holding this role key (tenant_owner, tenant_admin,
        /// tenant_member, or a custom role key).
        #[arg(long)]
        role: Option<String>,
        /// Only members with this email address (exact, case-insensitive).
        /// Repeat, or separate with commas, for several. Contact details are
        /// visible only to owners and admins.
        #[arg(long = "email", value_delimiter = ',')]
        emails: Vec<String>,
        /// Only human members or only virtual members.
        #[arg(long = "type", value_parser = ["human", "virtual"])]
        member_type: Option<String>,
    },
    /// Get one member by principal id.
    Get {
        /// Member principal id (from `member list`).
        principal_id: String,
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
    /// Change a member's role and/or usage cap. Only the flags given change.
    /// The owner role cannot be assigned, and neither the owner's role nor
    /// your own can be changed.
    Update {
        /// Member principal id (from `member list`).
        principal_id: String,
        /// New role: tenant_admin, tenant_member, or a custom role key.
        #[arg(long)]
        role: Option<String>,
        /// Per-member usage cap in tokens; 0 removes the cap.
        #[arg(long, value_parser = clap::value_parser!(i64).range(0..=MAX_MEMBER_TOKENS))]
        max_tokens: Option<i64>,
        /// Retrying with the same value replays the first result.
        #[arg(long)]
        idempotency_key: Option<String>,
    },
    /// Change a member's role. Shorthand for `member update --role`.
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
    // An empty update is a caller mistake; say so before asking for
    // credentials.
    if let MemberCommand::Update {
        role: None,
        max_tokens: None,
        ..
    } = &command
    {
        bail!("nothing to update; pass --role or --max-tokens");
    }

    let client = api_client(profile, base_url)?;

    match command {
        MemberCommand::List {
            page_size,
            continuation_token,
            name_fuzzy,
            role,
            emails,
            member_type,
        } => {
            let data = list_members(
                &client,
                &ListMembersParams {
                    page_size,
                    continuation_token,
                    name_fuzzy,
                    role,
                    emails,
                    member_type,
                },
            )
            .context("list members")?;
            print_json(&data)
        }
        MemberCommand::Get { principal_id } => {
            let data = get_member(&client, &principal_id).context("get member")?;
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
        MemberCommand::Update {
            principal_id,
            role,
            max_tokens,
            idempotency_key,
        } => {
            update_member(
                &client,
                &principal_id,
                &UpdateMemberRequest { role, max_tokens },
                idempotency_key.as_deref(),
            )
            .context("update member")?;
            // The endpoint answers with no payload; show the member as it
            // now stands rather than echoing what was asked for.
            let data = get_member(&client, &principal_id).context("read the updated member")?;
            print_json(&data)
        }
        MemberCommand::SetRole {
            principal_id,
            role,
            idempotency_key,
        } => {
            update_member(
                &client,
                &principal_id,
                &UpdateMemberRequest {
                    role: Some(role.clone()),
                    max_tokens: None,
                },
                idempotency_key.as_deref(),
            )
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
