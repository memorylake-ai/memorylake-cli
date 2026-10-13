//! `memorylake access` commands: who can reach one resource instance.

use anyhow::{Context, Result, bail};
use clap::{Args, Subcommand, ValueEnum};
use memorylake_core::api::admin::{
    ResourceKind, ResourceRef, Subject, add_scope_grant, get_permission_level_options,
    get_resource_access, remove_scope_grant, set_permission_level,
};

use memorylake_core::{Client, Paths, ResolveOverrides, resolve};

use super::{parse_non_blank, parse_reason, print_json, require_workspace};

/// Access subcommands.
///
/// Each takes the resource TYPE and ID. Workspace actors and agents live in
/// one workspace: `--workspace`, else the one `workspace use` remembered,
/// as for `actor bind`. Projects accept `--workspace` so the project shows
/// by name in access scopes; it is never filled in for them, since a wrong
/// guess would mislabel the project.
#[derive(Debug, Subcommand)]
pub enum AccessCommand {
    /// List every member and access group that can access the instance and
    /// why, the members who cannot, and the levels you may set.
    Show {
        #[command(flatten)]
        target: Target,
    },
    /// Show a member's permission level on the instance and the levels you
    /// may set them to.
    Levels {
        #[command(flatten)]
        target: Target,
        /// Member principal id (from `member list`).
        #[arg(long, value_name = "PRINCIPAL_ID", value_parser = parse_non_blank)]
        member: String,
    },
    /// Set a member's final level on the instance. `none` removes the level
    /// set here — it is not a deny: the member keeps what their role, scope,
    /// or access groups give them. The last member who can manage the
    /// instance cannot be lowered.
    SetLevel {
        #[command(flatten)]
        target: Target,
        /// Member principal id (from `member list`). Access groups cannot hold
        /// levels.
        #[arg(long, value_name = "PRINCIPAL_ID", value_parser = parse_non_blank)]
        member: String,
        /// The member's final level here.
        #[arg(long, value_parser = ["read", "write", "manage", "none"])]
        level: String,
        /// Why the change is made; recorded in the audit log.
        #[arg(long, value_parser = parse_reason)]
        reason: String,
        /// Retrying with the same value replays the first result.
        #[arg(long, value_parser = parse_non_blank)]
        idempotency_key: Option<String>,
    },
    /// Add the instance to a limited member's access scope, or to an access
    /// group's instance list. Repeating it changes nothing.
    Grant {
        #[command(flatten)]
        target: Target,
        #[command(flatten)]
        subject: SubjectArgs,
        /// Why the change is made; recorded in the audit log.
        #[arg(long, value_parser = parse_reason)]
        reason: String,
        /// Retrying with the same value replays the first result.
        #[arg(long, value_parser = parse_non_blank)]
        idempotency_key: Option<String>,
    },
    /// Remove the instance from a member's access scope or an access group's
    /// instance list. Removing a limited member's last instance is refused:
    /// it would lift the limit instead.
    Revoke {
        #[command(flatten)]
        target: Target,
        #[command(flatten)]
        subject: SubjectArgs,
        /// Why; recorded in the audit log.
        #[arg(long, value_parser = parse_reason)]
        reason: Option<String>,
        /// Retrying with the same value replays the first result.
        #[arg(long, value_parser = parse_non_blank)]
        idempotency_key: Option<String>,
    },
}

/// A resource type the `access` commands accept.
#[derive(Debug, Clone, Copy, PartialEq, Eq, ValueEnum)]
pub enum Kind {
    /// A workspace.
    Workspace,
    /// A project.
    Project,
    /// An actor (team-wide).
    Actor,
    /// An agent (team-wide).
    Agent,
    /// A memory boundary.
    Boundary,
    /// A database connection.
    DbConnection,
    /// A database datasource.
    DbDatasource,
    /// A skill.
    Skill,
    /// An actor's binding in one workspace (needs --workspace).
    WorkspaceActor,
    /// An agent's binding in one workspace (needs --workspace).
    WorkspaceAgent,
}

impl From<Kind> for ResourceKind {
    fn from(kind: Kind) -> Self {
        match kind {
            Kind::Workspace => Self::Workspace,
            Kind::Project => Self::Project,
            Kind::Actor => Self::Actor,
            Kind::Agent => Self::Agent,
            Kind::Boundary => Self::Boundary,
            Kind::DbConnection => Self::DbConnection,
            Kind::DbDatasource => Self::DbDatasource,
            Kind::Skill => Self::Skill,
            Kind::WorkspaceActor => Self::WorkspaceActor,
            Kind::WorkspaceAgent => Self::WorkspaceAgent,
        }
    }
}

/// The instance an `access` command is about.
#[derive(Debug, Args)]
pub struct Target {
    /// Resource type.
    #[arg(value_enum)]
    kind: Kind,
    /// Instance id, as the data API returns it: from `workspace list`,
    /// `project list`, `actor list`, `agent list`, `boundary list`,
    /// `dbconn list`, `ds list` or `skill list`. For workspace-actor and
    /// workspace-agent, the actor or agent id.
    #[arg(value_parser = parse_non_blank)]
    id: String,
    /// The workspace. For workspace-actor and workspace-agent: the binding's
    /// workspace, defaulting to the one `workspace use` remembered. For
    /// project: optional, shows the project by name in access scopes.
    /// Refused for the other types.
    #[arg(long, value_parser = parse_non_blank)]
    workspace: Option<String>,
}

impl Target {
    /// Check the `--workspace` rule and build the core reference. A
    /// workspace binding without --workspace is completed later, from the
    /// remembered workspace.
    fn resolve(&self) -> Result<ResourceRef> {
        let kind = ResourceKind::from(self.kind);
        if self.workspace.is_some() && !kind.needs_workspace() && !kind.takes_workspace_hint() {
            let name = self
                .kind
                .to_possible_value()
                .map(|value| value.get_name().to_string())
                .unwrap_or_default();
            bail!(
                "{name} takes no --workspace; only project, workspace-actor and workspace-agent do"
            );
        }
        Ok(ResourceRef {
            kind,
            id: self.id.clone(),
            workspace_id: self.workspace.clone(),
        })
    }
}

/// Who a scope grant is for.
#[derive(Debug, Args)]
#[group(required = true, multiple = false)]
pub struct SubjectArgs {
    /// A member (from `member list`), whose access scope must already be
    /// limited to listed instances of this type (see `member scope get`).
    #[arg(long, value_name = "PRINCIPAL_ID", value_parser = parse_non_blank)]
    member: Option<String>,
    /// An access group (from `access-group list`).
    #[arg(long, value_name = "GROUP_ID", value_parser = parse_non_blank)]
    group: Option<String>,
}

impl SubjectArgs {
    fn subject(&self) -> Subject {
        match (&self.member, &self.group) {
            (Some(member), _) => Subject::Member(member.clone()),
            (None, Some(group)) => Subject::AccessGroup(group.clone()),
            (None, None) => unreachable!("clap requires one subject"),
        }
    }
}

/// Execute an `access` subcommand.
pub fn run(
    command: AccessCommand,
    profile: Option<String>,
    base_url: Option<String>,
) -> Result<()> {
    // The --workspace rule is local; check it before asking for credentials.
    let resource = match &command {
        AccessCommand::Show { target }
        | AccessCommand::Levels { target, .. }
        | AccessCommand::SetLevel { target, .. }
        | AccessCommand::Grant { target, .. }
        | AccessCommand::Revoke { target, .. } => target.resolve()?,
    };

    let paths = Paths::default_home().context("resolve MemoryLake config paths")?;
    let runtime = resolve(&paths, &ResolveOverrides { profile, base_url })
        .context("resolve API credentials")?;
    let client = Client::new(&runtime.base_url, &runtime.api_key).context("build API client")?;
    let mut resource = resource;
    if resource.kind.needs_workspace() && resource.workspace_id.is_none() {
        resource.workspace_id = Some(require_workspace(&paths, &runtime.profile, None)?);
    }

    let data = match command {
        AccessCommand::Show { .. } => {
            get_resource_access(&client, &resource).context("show who can access it")?
        }
        AccessCommand::Levels { member, .. } => {
            let data = get_permission_level_options(&client, &resource, &member)
                .context("get permission level options")?;
            return print_json(&data);
        }
        AccessCommand::SetLevel {
            member,
            level,
            reason,
            idempotency_key,
            ..
        } => set_permission_level(
            &client,
            &resource,
            &member,
            &level,
            &reason,
            idempotency_key.as_deref(),
        )
        .context("set permission level")?,
        AccessCommand::Grant {
            subject,
            reason,
            idempotency_key,
            ..
        } => add_scope_grant(
            &client,
            &resource,
            &subject.subject(),
            &reason,
            idempotency_key.as_deref(),
        )
        .context("grant access")?,
        AccessCommand::Revoke {
            subject,
            reason,
            idempotency_key,
            ..
        } => remove_scope_grant(
            &client,
            &resource,
            &subject.subject(),
            reason.as_deref(),
            idempotency_key.as_deref(),
        )
        .context("revoke access")?,
    };
    print_json(&data)
}
