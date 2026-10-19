//! MemoryLake command-line interface.

mod commands;
mod interactive;

use std::io::IsTerminal;

use anyhow::Result;
use clap::{ArgAction, Parser, Subcommand};
use tracing::Level;
use tracing_subscriber::EnvFilter;

use commands::access::{AccessCommand, run as run_access};
use commands::access_group::{AccessGroupCommand, run as run_access_group};
use commands::actor::{ActorCommand, run as run_actor};
use commands::agent::{AgentCommand, run as run_agent};
use commands::agent_skills::{AgentSkillsCommand, run as run_agent_skills};
use commands::api_key::{ApiKeyCommand, run as run_api_key};
use commands::audit_log::{AuditLogCommand, run as run_audit_log};
use commands::auth::{AuthCommand, run as run_auth};
use commands::billing::{BillingCommand, run as run_billing};
use commands::boundary::{BoundaryCommand, run as run_boundary};
use commands::conversation::{ConversationCommand, run as run_conversation};
use commands::datasource::{DatasourceCommand, run as run_datasource};
use commands::db_connection::{DbConnectionCommand, run as run_db_connection};
use commands::fact::{FactCommand, run as run_fact};
use commands::integration::{IntegrationCommand, run as run_integration};
use commands::invitation::{InvitationCommand, run as run_invitation};
use commands::library::{LibraryCommand, run as run_library};
use commands::member::{MemberCommand, run as run_member};
use commands::opendata::{OpendataCommand, run as run_opendata};
use commands::permission::{PermissionCommand, run as run_permission};
use commands::project::{ProjectCommand, run as run_project};
use commands::role::{RoleCommand, run as run_role};
use commands::router::{RouterCommand, run as run_router};
use commands::search::{SearchArgs, run as run_search};
use commands::skill::{SkillCommand, run as run_skill};
use commands::team::{TeamCommand, run as run_team};
use commands::usage::{UsageArgs, run as run_usage};
use commands::workspace::{WorkspaceCommand, run as run_workspace};

/// What `--version` and `version` report.
///
/// Release builds are stamped with their tag by the release workflow, because
/// the crate version alone cannot answer "which build is this?" — it has stayed
/// at 0.1.0 across every release, so it could not tell an upgraded install from
/// a stale one. A build without the stamp says so rather than claiming a
/// release it is not.
const VERSION: &str = match option_env!("MEMORYLAKE_RELEASE") {
    Some(release) => release,
    None => concat!(env!("CARGO_PKG_VERSION"), " (dev build)"),
};

#[derive(Debug, Parser)]
#[command(
    name = "memorylake",
    version = VERSION,
    about = "Command-line interface for MemoryLake"
)]
struct Cli {
    /// Increase logging verbosity (`-v`, `-vv`).
    #[arg(short, long, action = ArgAction::Count, global = true)]
    verbose: u8,

    /// Profile to use for API commands.
    #[arg(long, global = true)]
    profile: Option<String>,

    /// Override the API base URL for this invocation.
    #[arg(long, global = true)]
    base_url: Option<String>,

    #[command(subcommand)]
    command: Commands,
}

#[derive(Debug, Subcommand)]
enum Commands {
    /// Manage authentication and profiles.
    Auth {
        #[command(subcommand)]
        command: AuthCommand,
    },
    /// Manage actors and their workspace bindings.
    Actor {
        #[command(subcommand)]
        command: ActorCommand,
    },
    /// Manage workspaces.
    #[command(visible_alias = "ws")]
    Workspace {
        #[command(subcommand)]
        command: WorkspaceCommand,
    },
    /// Manage projects within a workspace.
    #[command(visible_alias = "proj")]
    Project {
        #[command(subcommand)]
        command: ProjectCommand,
    },
    /// Browse and manage Library files and folders.
    #[command(visible_alias = "lib")]
    Library {
        #[command(subcommand)]
        command: LibraryCommand,
    },
    /// Manage agents, their versions, and their workspace bindings.
    Agent {
        #[command(subcommand)]
        command: AgentCommand,
    },
    /// Install the skills bundled with this CLI into coding agents (Claude
    /// Code, Codex, Gemini CLI, opencode), so they know how to drive it.
    AgentSkills {
        #[command(subcommand)]
        command: AgentSkillsCommand,
    },
    /// Publish and manage skills agents can use.
    Skill {
        #[command(subcommand)]
        command: SkillCommand,
    },
    /// Manage saved search scopes (boundaries) within a workspace.
    Boundary {
        #[command(subcommand)]
        command: BoundaryCommand,
    },
    /// Manage conversations and their messages.
    #[command(visible_alias = "conv")]
    Conversation {
        #[command(subcommand)]
        command: ConversationCommand,
    },
    /// Manage memory facts, their conflicts, and fact instructions.
    Fact {
        #[command(subcommand)]
        command: FactCommand,
    },
    /// Manage saved database connections (host, credentials).
    #[command(visible_alias = "dbconn")]
    DbConnection {
        #[command(subcommand)]
        command: DbConnectionCommand,
    },
    /// Manage database datasources: one indexed schema of a connection,
    /// in a workspace.
    #[command(visible_alias = "ds")]
    Datasource {
        #[command(subcommand)]
        command: DatasourceCommand,
    },
    /// Search memories in a workspace.
    Search(SearchArgs),
    /// Call OpenAI- and Anthropic-compatible models through MemoryRouter,
    /// with a Boundary's memory attached.
    Router {
        #[command(subcommand)]
        command: RouterCommand,
    },
    /// List the opendata a project can draw on.
    Opendata {
        #[command(subcommand)]
        command: OpendataCommand,
    },
    /// Show, rename, create, delete, or hand over teams.
    Team {
        #[command(subcommand)]
        command: TeamCommand,
    },
    /// Manage the team's API keys.
    #[command(visible_alias = "key")]
    ApiKey {
        #[command(subcommand)]
        command: ApiKeyCommand,
    },
    /// Manage the team roster and virtual members.
    Member {
        #[command(subcommand)]
        command: MemberCommand,
    },
    /// List, inspect, and manage roles and what holders of a role reach.
    Role {
        #[command(subcommand)]
        command: RoleCommand,
    },
    /// Manage access groups: extra permissions and instances for their
    /// members.
    #[command(visible_alias = "ag")]
    AccessGroup {
        #[command(subcommand)]
        command: AccessGroupCommand,
    },
    /// Show your permissions, the permission catalog, and permission rules.
    #[command(visible_alias = "perm")]
    Permission {
        #[command(subcommand)]
        command: PermissionCommand,
    },
    /// See and change who can reach one resource instance.
    Access {
        #[command(subcommand)]
        command: AccessCommand,
    },
    /// Invite people to the team and manage pending invitations.
    #[command(visible_alias = "invite")]
    Invitation {
        #[command(subcommand)]
        command: InvitationCommand,
    },
    /// Show the team's quota, usage, and individual usage records.
    Usage(UsageArgs),
    /// Show the team's credit, invoices, orders, and subscription; set the
    /// low-balance alert and invoice buyer details.
    Billing {
        #[command(subcommand)]
        command: BillingCommand,
    },
    /// List the team's audit events.
    #[command(visible_alias = "audit")]
    AuditLog {
        #[command(subcommand)]
        command: AuditLogCommand,
    },
    /// Connect DingTalk, WeCom, Feishu, or Slack bots to agents.
    #[command(visible_alias = "im")]
    Integration {
        #[command(subcommand)]
        command: IntegrationCommand,
    },
    /// Print the CLI version.
    Version,
}

fn main() -> Result<()> {
    let cli = Cli::parse();
    init_tracing(cli.verbose)?;

    match cli.command {
        Commands::Auth { command } => run_auth(command, cli.profile, cli.base_url)?,
        Commands::Actor { command } => run_actor(command, cli.profile, cli.base_url)?,
        Commands::Workspace { command } => run_workspace(command, cli.profile, cli.base_url)?,
        Commands::Project { command } => run_project(command, cli.profile, cli.base_url)?,
        Commands::Library { command } => run_library(command, cli.profile, cli.base_url)?,
        Commands::Agent { command } => run_agent(command, cli.profile, cli.base_url)?,
        Commands::AgentSkills { command } => run_agent_skills(command)?,
        Commands::Skill { command } => run_skill(command, cli.profile, cli.base_url)?,
        Commands::Boundary { command } => run_boundary(command, cli.profile, cli.base_url)?,
        Commands::Conversation { command } => run_conversation(command, cli.profile, cli.base_url)?,
        Commands::Fact { command } => run_fact(command, cli.profile, cli.base_url)?,
        Commands::DbConnection { command } => {
            run_db_connection(command, cli.profile, cli.base_url)?
        }
        Commands::Datasource { command } => run_datasource(command, cli.profile, cli.base_url)?,
        Commands::Search(args) => run_search(args, cli.profile, cli.base_url)?,
        Commands::Router { command } => run_router(command, cli.profile, cli.base_url)?,
        Commands::Opendata { command } => run_opendata(command, cli.profile, cli.base_url)?,
        Commands::Team { command } => run_team(command, cli.profile, cli.base_url)?,
        Commands::ApiKey { command } => run_api_key(command, cli.profile, cli.base_url)?,
        Commands::Member { command } => run_member(command, cli.profile, cli.base_url)?,
        Commands::Role { command } => run_role(command, cli.profile, cli.base_url)?,
        Commands::AccessGroup { command } => run_access_group(command, cli.profile, cli.base_url)?,
        Commands::Permission { command } => run_permission(command, cli.profile, cli.base_url)?,
        Commands::Access { command } => run_access(command, cli.profile, cli.base_url)?,
        Commands::Invitation { command } => run_invitation(command, cli.profile, cli.base_url)?,
        Commands::Usage(args) => run_usage(args, cli.profile, cli.base_url)?,
        Commands::Billing { command } => run_billing(command, cli.profile, cli.base_url)?,
        Commands::AuditLog { command } => run_audit_log(command, cli.profile, cli.base_url)?,
        Commands::Integration { command } => run_integration(command, cli.profile, cli.base_url)?,
        Commands::Version => {
            println!("{VERSION}");
        }
    }

    Ok(())
}

fn init_tracing(verbose: u8) -> Result<()> {
    let level = match verbose {
        0 => Level::WARN,
        1 => Level::INFO,
        2 => Level::DEBUG,
        _ => Level::TRACE,
    };

    let filter = EnvFilter::builder()
        .with_default_directive(level.into())
        .from_env_lossy();

    // Diagnostics go to stderr so `-v` or `RUST_LOG` never corrupts the JSON
    // on stdout; colour only when a person is reading them.
    tracing_subscriber::fmt()
        .with_env_filter(filter)
        .with_target(false)
        .with_writer(std::io::stderr)
        .with_ansi(std::io::stderr().is_terminal())
        .try_init()
        .map_err(|err| anyhow::anyhow!(err))?;

    Ok(())
}
