//! `memorylake role` commands.

use anyhow::{Context, Result, bail};
use clap::Subcommand;
use memorylake_core::api::admin::{
    CreateRoleRequest, UpdateRoleRequest, create_role, delete_role, get_role, get_role_scope,
    list_roles, set_role_scope, update_role,
};

use super::scope_input::{SCOPE_SET_HELP, ScopeSetArgs};
use super::{api_client, parse_non_blank, print_json, print_json_with_etag};

/// Role subcommands.
#[derive(Debug, Subcommand)]
pub enum RoleCommand {
    /// List the team's roles: built-ins first, then custom roles. The `key`
    /// is what `member` and `invitation` commands accept as `--role`.
    List,
    /// Get one role, built-in or custom. Its `permissions` (each with where
    /// it comes from) appear only for callers who may manage policies.
    Get {
        /// Role key (from `role list`).
        #[arg(value_parser = parse_non_blank)]
        key: String,
    },
    /// Create a custom role. Only the team owner may manage roles. What it
    /// grants is then set with `permission rule create --role <key>`.
    Create {
        /// Display name.
        #[arg(long, value_parser = parse_non_blank)]
        label: String,
        /// Description.
        #[arg(long)]
        description: Option<String>,
        /// Start with a copy of this role's permission rules: tenant_admin,
        /// tenant_member, or a custom role key from `role list`. The copy is independent —
        /// later changes to either role do not reach the other.
        #[arg(long = "base-role", value_name = "ROLE_KEY")]
        base_role_key: Option<String>,
        /// Retrying with the same value replays the first result.
        #[arg(long, value_parser = parse_non_blank)]
        idempotency_key: Option<String>,
    },
    /// Rename a custom role or change its description. Built-in roles cannot
    /// change.
    Update {
        /// Custom role key.
        #[arg(value_parser = parse_non_blank)]
        key: String,
        /// New display name.
        #[arg(long, value_parser = parse_non_blank)]
        label: Option<String>,
        /// New description; "" clears it.
        #[arg(long)]
        description: Option<String>,
        /// Retrying with the same value replays the first result.
        #[arg(long, value_parser = parse_non_blank)]
        idempotency_key: Option<String>,
    },
    /// Delete a custom role with its permission rules and access scope.
    /// Refused while any member holds it. There is no confirmation prompt.
    Delete {
        /// Custom role key.
        #[arg(value_parser = parse_non_blank)]
        key: String,
        /// Retrying with the same value replays the first result.
        #[arg(long, value_parser = parse_non_blank)]
        idempotency_key: Option<String>,
    },
    /// Show or limit which instances holders of a role reach.
    Scope {
        #[command(subcommand)]
        command: ScopeCommand,
    },
}

/// `role scope` subcommands.
#[derive(Debug, Subcommand)]
pub enum ScopeCommand {
    /// Show, per resource type, whether holders reach every instance or only
    /// the listed ones. The owner role has no scope. The ETag (on standard
    /// error) is what `scope set --if-match` takes.
    Get {
        /// Role key.
        #[arg(value_parser = parse_non_blank)]
        key: String,
    },
    /// Replace the scope of each listed resource type for everyone holding
    /// the role. A scope limits; it never grants more than the role allows.
    /// Project instances need their workspace_id.
    #[command(after_long_help = SCOPE_SET_HELP)]
    Set {
        /// tenant_admin, tenant_member, or a custom role key.
        #[arg(value_parser = parse_non_blank)]
        key: String,
        #[command(flatten)]
        input: ScopeSetArgs,
    },
}

/// Execute a `role` subcommand.
pub fn run(command: RoleCommand, profile: Option<String>, base_url: Option<String>) -> Result<()> {
    if let RoleCommand::Update {
        label: None,
        description: None,
        ..
    } = &command
    {
        bail!("nothing to update; pass --label or --description");
    }
    let scope_request = match &command {
        RoleCommand::Scope {
            command: ScopeCommand::Set { input, .. },
        } => Some(input.request()?),
        _ => None,
    };

    let client = api_client(profile, base_url)?;

    match command {
        RoleCommand::List => {
            let data = list_roles(&client).context("list roles")?;
            print_json(&data)
        }
        RoleCommand::Get { key } => {
            let data = get_role(&client, &key).with_context(|| format!("get role `{key}`"))?;
            print_json(&data)
        }
        RoleCommand::Create {
            label,
            description,
            base_role_key,
            idempotency_key,
        } => {
            let data = create_role(
                &client,
                &CreateRoleRequest {
                    label,
                    description,
                    base_role_key,
                },
                idempotency_key.as_deref(),
            )
            .context("create role")?;
            print_json(&data)
        }
        RoleCommand::Update {
            key,
            label,
            description,
            idempotency_key,
        } => {
            let data = update_role(
                &client,
                &key,
                &UpdateRoleRequest { label, description },
                idempotency_key.as_deref(),
            )
            .with_context(|| format!("update role `{key}`"))?;
            print_json(&data)
        }
        RoleCommand::Delete {
            key,
            idempotency_key,
        } => {
            delete_role(&client, &key, idempotency_key.as_deref())
                .with_context(|| format!("delete role `{key}`"))?;
            println!("Deleted role `{key}`");
            Ok(())
        }
        RoleCommand::Scope { command } => match command {
            ScopeCommand::Get { key } => {
                let (data, etag) = get_role_scope(&client, &key)
                    .with_context(|| format!("get the scope of role `{key}`"))?;
                print_json_with_etag(&data, etag.as_deref())
            }
            ScopeCommand::Set { key, input } => {
                let request = scope_request.expect("parsed above");
                let data = set_role_scope(&client, &key, &request, input.if_match())
                    .with_context(|| format!("set the scope of role `{key}`"))?;
                print_json(&data)
            }
        },
    }
}
