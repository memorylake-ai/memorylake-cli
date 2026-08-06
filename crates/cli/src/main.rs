//! MemoryLake command-line interface.

mod commands;
mod interactive;

use anyhow::Result;
use clap::{ArgAction, Parser, Subcommand};
use tracing::Level;
use tracing_subscriber::EnvFilter;

use commands::actor::{ActorCommand, run as run_actor};
use commands::auth::{AuthCommand, run as run_auth};
use commands::workspace::{WorkspaceCommand, run as run_workspace};

#[derive(Debug, Parser)]
#[command(
    name = "memorylake",
    version,
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
        Commands::Version => {
            println!("{}", env!("CARGO_PKG_VERSION"));
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
