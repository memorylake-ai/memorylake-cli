//! `memorylake permission` / `perm` commands.

use anyhow::{Context, Result};
use clap::{Args, Subcommand};
use memorylake_core::api::admin::{
    CreatePermissionRulesRequest, ListPermissionRulesParams, create_permission_rules,
    delete_permission_rule, get_my_permissions, get_permission_catalog, list_permission_rules,
};

use super::{api_client, parse_non_blank, parse_reason, print_json};

/// Permission subcommands.
#[derive(Debug, Subcommand)]
pub enum PermissionCommand {
    /// Show what you (the key's owner) may do in this team: every permission
    /// held, and per resource type whether it reaches every instance.
    Me,
    /// List the resource types and actions rules can name, with each
    /// action's prerequisites.
    Catalog {
        /// Only what can be granted to an access group.
        #[arg(long)]
        access_group: bool,
    },
    /// List, create, or delete permission rules on custom roles and members.
    Rule {
        #[command(subcommand)]
        command: RuleCommand,
    },
}

/// `permission rule` subcommands.
#[derive(Debug, Subcommand)]
pub enum RuleCommand {
    /// List the rules on roles (built-ins included) and members. Scopes,
    /// permission levels, and access-group permissions are not rules; see
    /// `member scope`, `access`, and `access-group`.
    List {
        /// Only rules of this subject type.
        #[arg(long, value_parser = ["role", "member"])]
        subject_type: Option<String>,
        /// Only rules of this role key or principal id.
        #[arg(long, requires = "subject_type")]
        subject_id: Option<String>,
        /// Number of items per page (1–100).
        #[arg(long)]
        page_size: Option<u32>,
        /// Continuation token from a previous response.
        #[arg(long)]
        continuation_token: Option<String>,
    },
    /// Allow actions on a resource for a custom role or one member — one
    /// rule per action. Only allow rules exist: to take a permission away,
    /// delete the rule granting it. Built-in roles cannot change.
    Create {
        #[command(flatten)]
        subject: RuleSubject,
        /// Resource type (see `permission catalog`), e.g. memorylake:project,
        /// or one instance as <resource_type>:<instance_id>.
        #[arg(long, value_parser = parse_non_blank)]
        resource: String,
        /// Action to allow, e.g. project:read. Repeat, or separate with
        /// commas, for several.
        #[arg(long = "action", required = true, value_delimiter = ',', value_parser = parse_non_blank)]
        actions: Vec<String>,
        /// Also grant actions these depend on that the subject lacks;
        /// without it the request fails naming what is missing.
        #[arg(long)]
        auto_grant_prerequisites: bool,
        /// Why the change is made; recorded in the audit log.
        #[arg(long, value_parser = parse_reason)]
        reason: String,
        /// Retrying with the same value replays the first result.
        #[arg(long)]
        idempotency_key: Option<String>,
    },
    /// Delete one rule by the rule_id `rule list` prints. Rules of the
    /// built-in roles cannot be deleted.
    Delete {
        /// Rule id.
        rule_id: String,
        /// Why; recorded in the audit log.
        #[arg(long, value_parser = parse_reason)]
        reason: Option<String>,
        /// Retrying with the same value replays the first result.
        #[arg(long)]
        idempotency_key: Option<String>,
    },
}

/// Who a new rule applies to.
#[derive(Debug, Args)]
#[group(required = true, multiple = false)]
pub struct RuleSubject {
    /// A custom role key.
    #[arg(long, value_name = "ROLE_KEY")]
    role: Option<String>,
    /// A member's principal id.
    #[arg(long, value_name = "PRINCIPAL_ID")]
    member: Option<String>,
}

/// Execute a `permission` subcommand.
pub fn run(
    command: PermissionCommand,
    profile: Option<String>,
    base_url: Option<String>,
) -> Result<()> {
    let client = api_client(profile, base_url)?;

    match command {
        PermissionCommand::Me => {
            let data = get_my_permissions(&client).context("get my permissions")?;
            print_json(&data)
        }
        PermissionCommand::Catalog { access_group } => {
            let data = get_permission_catalog(&client, access_group)
                .context("get the permission catalog")?;
            print_json(&data)
        }
        PermissionCommand::Rule { command } => match command {
            RuleCommand::List {
                subject_type,
                subject_id,
                page_size,
                continuation_token,
            } => {
                let data = list_permission_rules(
                    &client,
                    &ListPermissionRulesParams {
                        subject_type,
                        subject_id,
                        page_size,
                        continuation_token,
                    },
                )
                .context("list permission rules")?;
                print_json(&data)
            }
            RuleCommand::Create {
                subject,
                resource,
                actions,
                auto_grant_prerequisites,
                reason,
                idempotency_key,
            } => {
                let (subject_type, subject_id) = match (subject.role, subject.member) {
                    (Some(role), _) => ("role", role),
                    (None, Some(member)) => ("member", member),
                    (None, None) => unreachable!("clap requires one subject"),
                };
                let data = create_permission_rules(
                    &client,
                    &CreatePermissionRulesRequest {
                        subject_type: subject_type.to_string(),
                        subject_id,
                        resource,
                        actions,
                        auto_grant_prerequisites,
                        reason,
                    },
                    idempotency_key.as_deref(),
                )
                .context("create permission rules")?;
                print_json(&data)
            }
            RuleCommand::Delete {
                rule_id,
                reason,
                idempotency_key,
            } => {
                delete_permission_rule(
                    &client,
                    &rule_id,
                    reason.as_deref(),
                    idempotency_key.as_deref(),
                )
                .context("delete permission rule")?;
                println!("permission rule {rule_id} deleted");
                Ok(())
            }
        },
    }
}
