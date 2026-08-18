//! CLI subcommand handlers.

pub mod actor;
pub mod agent;
pub mod auth;
pub mod conversation;
pub mod fact;
pub mod library;

pub mod project;
pub mod search;
pub mod workspace;

use anyhow::{Result, bail};
use memorylake_core::{Paths, resolve_profile_workspace};

/// Resolve the workspace a command should act on.
///
/// Precedence is `--workspace` → the profile's remembered workspace → the
/// `MEMORYLAKE_WORKSPACE` environment variable. Commands that need a workspace
/// call this instead of reading their flag directly, so every one of them
/// honours `workspace use` the same way.
///
/// Deliberately not applied to `actor list --workspace`, where the flag
/// switches what is listed rather than naming where to look: defaulting it
/// would silently turn "list every actor" into "list this workspace's
/// bindings".
pub fn require_workspace(paths: &Paths, profile: &str, flag: Option<String>) -> Result<String> {
    match resolve_profile_workspace(paths, profile, flag.as_deref())? {
        Some((workspace, _)) => Ok(workspace),
        None => bail!(
            "no workspace given and none remembered for profile `{profile}`\n\
             pick one to reuse:  memorylake workspace use\n\
             or name it here:    --workspace <id>"
        ),
    }
}
