//! `memorylake access-group` / `ag` commands.

use anyhow::{Context, Result, bail};
use clap::Subcommand;
use memorylake_core::api::admin::{
    AccessGroupPermission, CreateAccessGroupRequest, UpdateAccessGroupRequest,
    add_access_group_members, create_access_group, delete_access_group, explain_access_group,
    get_access_group, get_access_group_scope, list_access_group_members, list_access_groups,
    remove_access_group_member, set_access_group_permissions, set_access_group_scope,
    update_access_group,
};

use super::scope_input::{SCOPE_SET_HELP, ScopeSetArgs};
use super::{
    api_client, parse_etag, parse_non_blank, parse_reason, print_json, print_json_with_etag,
};

/// Most members one `access-group member add` may name.
const MAX_MEMBERS_PER_ADD: usize = 200;

/// Access group subcommands.
///
/// An access group gives its members extra permissions and access to extra
/// instances; it never takes access away. See `access` for adding single
/// instances to a group.
#[derive(Debug, Subcommand)]
pub enum AccessGroupCommand {
    /// List the team's access groups.
    List {
        /// Number of items per page (1–100).
        #[arg(long, value_parser = clap::value_parser!(u32).range(1..=100))]
        page_size: Option<u32>,
        /// Continuation token from a previous response.
        #[arg(long)]
        continuation_token: Option<String>,
    },
    /// Get one access group. Its ETag (on standard error) is what
    /// `permissions set --if-match` takes.
    Get {
        /// Access group id (from `access-group list`).
        #[arg(value_parser = parse_non_blank)]
        id: String,
    },
    /// Create an access group: an optional base (a role or another group,
    /// fixed at creation) plus its own permissions.
    Create {
        /// Display name.
        #[arg(long, value_parser = parse_non_blank)]
        name: String,
        /// Description.
        #[arg(long)]
        description: Option<String>,
        /// Grant everything this role grants. tenant_admin needs an owner or
        /// admin.
        #[arg(long, value_parser = ["tenant_member", "tenant_admin"], conflicts_with = "base_group")]
        base_role: Option<String>,
        /// Grant everything this other access group grants.
        #[arg(long, value_name = "GROUP_ID")]
        base_group: Option<String>,
        /// A permission on top of the base, as <resource_type>:<action>, e.g.
        /// memorylake:actor:actor:create. Repeat for several; see `permission
        /// catalog --access-group` for what is accepted.
        #[arg(long = "permission", value_name = "PERMISSION", value_parser = AccessGroupPermission::parse)]
        permissions: Vec<AccessGroupPermission>,
        /// Retrying with the same value replays the first result.
        #[arg(long, value_parser = parse_non_blank)]
        idempotency_key: Option<String>,
    },
    /// Rename an access group or change its description. The base cannot
    /// change.
    Update {
        /// Access group id (from `access-group list`).
        #[arg(value_parser = parse_non_blank)]
        id: String,
        /// New display name.
        #[arg(long, value_parser = parse_non_blank)]
        name: Option<String>,
        /// New description; "" clears it.
        #[arg(long)]
        description: Option<String>,
        /// Retrying with the same value replays the first result.
        #[arg(long, value_parser = parse_non_blank)]
        idempotency_key: Option<String>,
    },
    /// Delete an access group; its members lose what it gave them. Refused
    /// while other groups are based on it. There is no confirmation prompt.
    Delete {
        /// Access group id (from `access-group list`).
        #[arg(value_parser = parse_non_blank)]
        id: String,
        /// Retrying with the same value replays the first result.
        #[arg(long, value_parser = parse_non_blank)]
        idempotency_key: Option<String>,
    },
    /// Explain everything the group grants and where each permission comes
    /// from, plus the instances it gives access to.
    Explain {
        /// Access group id (from `access-group list`).
        #[arg(value_parser = parse_non_blank)]
        id: String,
    },
    /// List, add, or remove the group's members.
    Member {
        #[command(subcommand)]
        command: MemberCommand,
    },
    /// Replace the group's own permissions.
    Permissions {
        #[command(subcommand)]
        command: PermissionsCommand,
    },
    /// Show or replace the instances the group gives access to.
    Scope {
        #[command(subcommand)]
        command: ScopeCommand,
    },
}

/// `access-group member` subcommands.
#[derive(Debug, Subcommand)]
pub enum MemberCommand {
    /// List the group's members.
    List {
        /// Access group id (from `access-group list`).
        #[arg(value_parser = parse_non_blank)]
        id: String,
        /// Number of items per page (1–100).
        #[arg(long, value_parser = clap::value_parser!(u32).range(1..=100))]
        page_size: Option<u32>,
        /// Continuation token from a previous response.
        #[arg(long)]
        continuation_token: Option<String>,
    },
    /// Add members, all or nothing: one id outside the team adds nobody.
    /// Someone already in the group is not an error.
    Add {
        /// Access group id (from `access-group list`).
        #[arg(value_parser = parse_non_blank)]
        id: String,
        /// Principal ids to add (1–200, from `member list`).
        #[arg(required = true, value_name = "PRINCIPAL_ID", value_parser = parse_non_blank)]
        principal_ids: Vec<String>,
        /// Retrying with the same value replays the first result.
        #[arg(long, value_parser = parse_non_blank)]
        idempotency_key: Option<String>,
    },
    /// Remove one member from the group.
    Remove {
        /// Access group id (from `access-group list`).
        #[arg(value_parser = parse_non_blank)]
        id: String,
        /// Principal id to remove.
        principal_id: String,
        /// Retrying with the same value replays the first result.
        #[arg(long, value_parser = parse_non_blank)]
        idempotency_key: Option<String>,
    },
}

/// `access-group permissions` subcommands.
#[derive(Debug, Subcommand)]
pub enum PermissionsCommand {
    /// Replace the group's own permission list. The base is unaffected.
    Set {
        /// Access group id (from `access-group list`).
        #[arg(value_parser = parse_non_blank)]
        id: String,
        /// A permission, as <resource_type>:<action>, e.g.
        /// memorylake:actor:actor:create. Repeat for several; see `permission
        /// catalog --access-group` for what is accepted.
        #[arg(
            long = "permission",
            value_name = "PERMISSION",
            value_parser = AccessGroupPermission::parse,
            required_unless_present = "none",
            conflicts_with = "none"
        )]
        permissions: Vec<AccessGroupPermission>,
        /// Remove every permission of the group's own list.
        #[arg(long)]
        none: bool,
        /// Why the change is made; recorded in the audit log.
        #[arg(long, value_parser = parse_reason)]
        reason: String,
        /// Only write if the list is unchanged since the `get` that printed
        /// this ETag (`ETag: "..."` on standard error; the value, with or
        /// without its quotes, or the whole line); otherwise fail with 412.
        #[arg(long, value_name = "ETAG", value_parser = parse_etag)]
        if_match: Option<String>,
    },
}

/// `access-group scope` subcommands.
#[derive(Debug, Subcommand)]
pub enum ScopeCommand {
    /// Show, per resource type, the instances the group gives access to.
    /// The ETag (on standard error) is what `scope set --if-match` takes.
    Get {
        /// Access group id (from `access-group list`).
        #[arg(value_parser = parse_non_blank)]
        id: String,
    },
    /// Replace, per listed resource type, the instances the group gives
    /// access to. Mode `all` removes the type's instances: a group never
    /// limits anyone. Project instances need their workspace_id.
    #[command(after_long_help = SCOPE_SET_HELP)]
    Set {
        /// Access group id (from `access-group list`).
        #[arg(value_parser = parse_non_blank)]
        id: String,
        #[command(flatten)]
        input: ScopeSetArgs,
    },
}

/// Execute an `access-group` subcommand.
pub fn run(
    command: AccessGroupCommand,
    profile: Option<String>,
    base_url: Option<String>,
) -> Result<()> {
    // Everything checkable locally is checked before credentials are needed.
    match &command {
        AccessGroupCommand::Update {
            name: None,
            description: None,
            ..
        } => bail!("nothing to update; pass --name or --description"),
        AccessGroupCommand::Member {
            command: MemberCommand::Add { principal_ids, .. },
        } if principal_ids.len() > MAX_MEMBERS_PER_ADD => bail!(
            "at most {MAX_MEMBERS_PER_ADD} members per call, got {}",
            principal_ids.len()
        ),
        _ => {}
    }
    let scope_request = match &command {
        AccessGroupCommand::Scope {
            command: ScopeCommand::Set { input, .. },
        } => Some(input.request()?),
        _ => None,
    };

    let client = api_client(profile, base_url)?;

    match command {
        AccessGroupCommand::List {
            page_size,
            continuation_token,
        } => {
            let data = list_access_groups(&client, page_size, continuation_token.as_deref())
                .context("list access groups")?;
            print_json(&data)
        }
        AccessGroupCommand::Get { id } => {
            let (data, etag) = get_access_group(&client, &id)
                .with_context(|| format!("get access group `{id}`"))?;
            print_json_with_etag(&data, etag.as_deref())
        }
        AccessGroupCommand::Create {
            name,
            description,
            base_role,
            base_group,
            permissions,
            idempotency_key,
        } => {
            let data = create_access_group(
                &client,
                &CreateAccessGroupRequest {
                    name,
                    description,
                    base_role_key: base_role,
                    base_access_group_id: base_group,
                    permissions: (!permissions.is_empty()).then_some(permissions),
                },
                idempotency_key.as_deref(),
            )
            .context("create access group")?;
            print_json(&data)
        }
        AccessGroupCommand::Update {
            id,
            name,
            description,
            idempotency_key,
        } => {
            let data = update_access_group(
                &client,
                &id,
                &UpdateAccessGroupRequest { name, description },
                idempotency_key.as_deref(),
            )
            .with_context(|| format!("update access group `{id}`"))?;
            print_json(&data)
        }
        AccessGroupCommand::Delete {
            id,
            idempotency_key,
        } => {
            delete_access_group(&client, &id, idempotency_key.as_deref())
                .with_context(|| format!("delete access group `{id}`"))?;
            println!("Deleted access group `{id}`");
            Ok(())
        }
        AccessGroupCommand::Explain { id } => {
            let data = explain_access_group(&client, &id)
                .with_context(|| format!("explain access group `{id}`"))?;
            print_json(&data)
        }
        AccessGroupCommand::Member { command } => match command {
            MemberCommand::List {
                id,
                page_size,
                continuation_token,
            } => {
                let data = list_access_group_members(
                    &client,
                    &id,
                    page_size,
                    continuation_token.as_deref(),
                )
                .context("list access group members")?;
                print_json(&data)
            }
            MemberCommand::Add {
                id,
                principal_ids,
                idempotency_key,
            } => {
                let data = add_access_group_members(
                    &client,
                    &id,
                    &principal_ids,
                    idempotency_key.as_deref(),
                )
                .context("add access group members")?;
                print_json(&data)
            }
            MemberCommand::Remove {
                id,
                principal_id,
                idempotency_key,
            } => {
                remove_access_group_member(&client, &id, &principal_id, idempotency_key.as_deref())
                    .with_context(|| {
                        format!("remove member `{principal_id}` from access group `{id}`")
                    })?;
                println!("Removed member `{principal_id}` from access group `{id}`");
                Ok(())
            }
        },
        AccessGroupCommand::Permissions {
            command:
                PermissionsCommand::Set {
                    id,
                    permissions,
                    none: _,
                    reason,
                    if_match,
                },
        } => {
            let data = set_access_group_permissions(
                &client,
                &id,
                &permissions,
                &reason,
                if_match.as_deref(),
            )
            .context("set access group permissions")?;
            print_json(&data)
        }
        AccessGroupCommand::Scope { command } => match command {
            ScopeCommand::Get { id } => {
                let (data, etag) =
                    get_access_group_scope(&client, &id).context("get access group scope")?;
                print_json_with_etag(&data, etag.as_deref())
            }
            ScopeCommand::Set { id, input } => {
                let request = scope_request.expect("parsed above");
                let data = set_access_group_scope(&client, &id, &request, input.if_match())
                    .context("set access group scope")?;
                print_json(&data)
            }
        },
    }
}
