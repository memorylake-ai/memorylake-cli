//! `memorylake member` commands.

use anyhow::{Context, Result, bail};
use clap::Subcommand;
use memorylake_core::api::admin::{
    CreateMemberRequest, ListMembersParams, MAX_MEMBER_TOKENS, UpdateMemberRequest, create_member,
    explain_member, get_member, get_member_scope, list_members, remove_member, set_member_scope,
    update_member,
};

use super::scope_input::ScopeSetArgs;
use super::{api_client, parse_non_blank, print_json, print_json_with_etag};

/// Member subcommands.
#[derive(Debug, Subcommand)]
pub enum MemberCommand {
    /// List the team roster. Contact details appear only for owners/admins.
    List {
        /// Number of items per page (1-100).
        #[arg(long, value_parser = clap::value_parser!(u32).range(1..=100))]
        page_size: Option<u32>,
        /// Continuation token from a previous response.
        #[arg(long)]
        continuation_token: Option<String>,
        /// Fuzzy filter by display name (owners/admins also match email and
        /// username).
        #[arg(long = "name")]
        name_fuzzy: Option<String>,
        /// Only members holding this role key (tenant_owner, tenant_admin,
        /// tenant_member, or a custom role key from `role list`).
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
        #[arg(value_parser = parse_non_blank)]
        principal_id: String,
    },
    /// Create a virtual member: a login-less identity that acts only through
    /// API keys issued for it (`api-key create --member`).
    Create {
        /// Display name of the virtual member.
        #[arg(long, value_parser = parse_non_blank)]
        name: String,
        /// Role: tenant_admin, tenant_member, or a custom role key from `role
        /// list`.
        #[arg(long, value_parser = parse_non_blank)]
        role: String,
        /// Retrying with the same value replays the first result.
        #[arg(long, value_parser = parse_non_blank)]
        idempotency_key: Option<String>,
    },
    /// Change a member's role and/or usage cap. Only the flags given change.
    /// The owner role cannot be assigned, and neither the owner's role nor
    /// your own can be changed.
    Update {
        /// Member principal id (from `member list`).
        #[arg(value_parser = parse_non_blank)]
        principal_id: String,
        /// New role: tenant_admin, tenant_member, or a custom role key from
        /// `role list`.
        #[arg(long, value_parser = parse_non_blank)]
        role: Option<String>,
        /// Per-member usage cap in tokens; 0 removes the cap.
        #[arg(long, value_parser = clap::value_parser!(i64).range(0..=MAX_MEMBER_TOKENS))]
        max_tokens: Option<i64>,
        /// Retrying with the same value replays the first result.
        #[arg(long, value_parser = parse_non_blank)]
        idempotency_key: Option<String>,
    },
    /// Change a member's role. Shorthand for `member update --role`.
    SetRole {
        /// Member principal id (from `member list`).
        #[arg(value_parser = parse_non_blank)]
        principal_id: String,
        /// New role: tenant_admin, tenant_member, or a custom role key from
        /// `role list`.
        #[arg(long, value_parser = parse_non_blank)]
        role: String,
        /// Retrying with the same value replays the first result.
        #[arg(long, value_parser = parse_non_blank)]
        idempotency_key: Option<String>,
    },
    /// Remove a member. Their API keys in this team are disabled, not
    /// deleted. The owner cannot be removed; neither can yourself. There is
    /// no confirmation prompt.
    #[command(visible_alias = "delete")]
    Remove {
        /// Member principal id (from `member list`).
        #[arg(value_parser = parse_non_blank)]
        principal_id: String,
        /// Retrying with the same value replays the first result.
        #[arg(long, value_parser = parse_non_blank)]
        idempotency_key: Option<String>,
    },
    /// Explain what a member can do and where each permission comes from:
    /// the role, access groups, or a direct grant.
    Explain {
        /// Member principal id (from `member list`).
        principal_id: String,
    },
    /// Show or limit which instances a member reaches.
    Scope {
        #[command(subcommand)]
        command: ScopeCommand,
    },
}

/// `member scope` subcommands.
#[derive(Debug, Subcommand)]
pub enum ScopeCommand {
    /// Show, per resource type, whether the member reaches every instance or
    /// only the listed ones. The ETag (on standard error) is what
    /// `scope set --if-match` takes.
    Get {
        /// Member principal id.
        principal_id: String,
    },
    /// Replace the scope of each listed resource type. A scope limits a
    /// member; it never grants more than their role allows, and a limited
    /// type must leave them at least one instance. Project instances need
    /// their workspace_id.
    Set {
        /// Member principal id.
        principal_id: String,
        #[command(flatten)]
        input: ScopeSetArgs,
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
    let scope_request = match &command {
        MemberCommand::Scope {
            command: ScopeCommand::Set { input, .. },
        } => Some(input.request()?),
        _ => None,
    };

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
            let data = get_member(&client, &principal_id)
                .with_context(|| format!("get member `{principal_id}`"))?;
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
            .with_context(|| format!("update member `{principal_id}`"))?;
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
            .with_context(|| format!("change the role of member `{principal_id}`"))?;
            // Same output as `member update --role`: the member as it now
            // stands.
            let data = get_member(&client, &principal_id).context("read the updated member")?;
            print_json(&data)
        }
        MemberCommand::Remove {
            principal_id,
            idempotency_key,
        } => {
            remove_member(&client, &principal_id, idempotency_key.as_deref())
                .with_context(|| format!("remove member `{principal_id}`"))?;
            println!("Removed member `{principal_id}`");
            Ok(())
        }
        MemberCommand::Explain { principal_id } => {
            let data = explain_member(&client, &principal_id).context("explain member")?;
            print_json(&data)
        }
        MemberCommand::Scope { command } => match command {
            ScopeCommand::Get { principal_id } => {
                let (data, etag) =
                    get_member_scope(&client, &principal_id).context("get member scope")?;
                print_json_with_etag(&data, etag.as_deref())
            }
            ScopeCommand::Set {
                principal_id,
                input,
            } => {
                let request = scope_request.expect("parsed above");
                let data = set_member_scope(&client, &principal_id, &request, input.if_match())
                    .context("set member scope")?;
                print_json(&data)
            }
        },
    }
}
