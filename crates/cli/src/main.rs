//! MemoryLake command-line interface.

mod commands;
mod interactive;

use anyhow::Result;
use clap::{ArgAction, Parser, Subcommand};
use tracing::Level;
use tracing_subscriber::EnvFilter;

use commands::actor::{ActorCommand, run as run_actor};
use commands::agent::{AgentCommand, run as run_agent};
use commands::api_key::{ApiKeyCommand, run as run_api_key};
use commands::auth::{AuthCommand, run as run_auth};
use commands::conversation::{ConversationCommand, run as run_conversation};
use commands::fact::{FactCommand, run as run_fact};
use commands::invitation::{InvitationCommand, run as run_invitation};
use commands::library::{LibraryCommand, run as run_library};
use commands::member::{MemberCommand, run as run_member};
use commands::project::{ProjectCommand, run as run_project};
use commands::role::{RoleCommand, run as run_role};
use commands::search::{SearchArgs, run as run_search};
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
    /// Manage conversations and their messages.
    #[command(visible_alias = "conv")]
    Conversation {
        #[command(subcommand)]
        command: ConversationCommand,
    },
    /// Add, delete, and list memory facts.
    Fact {
        #[command(subcommand)]
        command: FactCommand,
    },
    /// Search memories in a workspace.
    Search(SearchArgs),
    /// Show and rename the team this API key belongs to.
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
    /// List the roles members and invitees can hold.
    Role {
        #[command(subcommand)]
        command: RoleCommand,
    },
    /// Invite people to the team and manage pending invitations.
    #[command(visible_alias = "invite")]
    Invitation {
        #[command(subcommand)]
        command: InvitationCommand,
    },
    /// Show the team's quota and usage.
    Usage(UsageArgs),
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
        Commands::Conversation { command } => run_conversation(command, cli.profile, cli.base_url)?,
        Commands::Fact { command } => run_fact(command, cli.profile, cli.base_url)?,
        Commands::Search(args) => run_search(args, cli.profile, cli.base_url)?,
        Commands::Team { command } => run_team(command, cli.profile, cli.base_url)?,
        Commands::ApiKey { command } => run_api_key(command, cli.profile, cli.base_url)?,
        Commands::Member { command } => run_member(command, cli.profile, cli.base_url)?,
        Commands::Role { command } => run_role(command, cli.profile, cli.base_url)?,
        Commands::Invitation { command } => run_invitation(command, cli.profile, cli.base_url)?,
        Commands::Usage(args) => run_usage(args, cli.profile, cli.base_url)?,
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

    tracing_subscriber::fmt()
        .with_env_filter(filter)
        .with_target(false)
        .try_init()
        .map_err(|err| anyhow::anyhow!(err))?;

    Ok(())
}
