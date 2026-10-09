//! The `--actor` / `--project` / `--agent` scope flags and the API session
//! shared by every scoped `fact` subcommand.

use anyhow::{Context, Result, bail};
use clap::Args;
use memorylake_core::api::facts::FactScope;
use memorylake_core::{Client, Paths, ResolveOverrides, resolve};

use crate::commands::require_workspace;

/// Workspace plus exactly one owning scope.
///
/// Facts, conflicts, and memory settings all live under one actor, project,
/// or agent, so these flags are shared by every subcommand that addresses a
/// single scope.
#[derive(Debug, Args)]
pub struct ScopeArgs {
    /// Workspace id the scope belongs to.
    ///
    /// Defaults to the workspace remembered by `workspace use`.
    #[arg(long)]
    pub workspace: Option<String>,
    /// Address this actor's memory. Exactly one of --actor / --project / --agent.
    #[arg(long, value_name = "ACTOR_ID")]
    pub actor: Option<String>,
    /// Address this project's memory. Exactly one of --actor / --project / --agent.
    #[arg(long, value_name = "PROJECT_ID")]
    pub project: Option<String>,
    /// Address this agent's memory. Exactly one of --actor / --project / --agent.
    #[arg(long, value_name = "AGENT_ID")]
    pub agent: Option<String>,
}

impl ScopeArgs {
    /// Validate the scope flags, returning the `--workspace` flag alongside.
    ///
    /// Purely local, so callers run it before resolving credentials.
    pub fn resolve(self) -> Result<(Option<String>, FactScope)> {
        let scope = resolve_scope(self.actor, self.project, self.agent)?;
        Ok((self.workspace, scope))
    }
}

/// Resolve the required, mutually exclusive `--actor` / `--project` /
/// `--agent` trio.
///
/// Enforced at runtime rather than through a clap group so the error can spell
/// out the scope model instead of a generic conflict message.
pub fn resolve_scope(
    actor: Option<String>,
    project: Option<String>,
    agent: Option<String>,
) -> Result<FactScope> {
    match (actor, project, agent) {
        (Some(actor_id), None, None) => Ok(FactScope::Actor(actor_id)),
        (None, Some(project_id), None) => Ok(FactScope::Project(project_id)),
        (None, None, Some(agent_id)) => Ok(FactScope::Agent(agent_id)),
        (None, None, None) => bail!(
            "a scope is required: --actor <id> for an actor's memory, \
             --project <id> for a project's, --agent <id> for an agent's"
        ),
        _ => bail!(
            "memory belongs to exactly one scope; pass one of --actor / --project / --agent, \
             not several"
        ),
    }
}

/// Resolved credentials and the client built from them.
pub struct Session {
    paths: Paths,
    profile: String,
    /// Authenticated API client.
    pub client: Client,
}

impl Session {
    /// Resolve credentials and build the client.
    pub fn open(profile: Option<String>, base_url: Option<String>) -> Result<Self> {
        let paths = Paths::default_home().context("resolve MemoryLake config paths")?;
        let runtime = resolve(&paths, &ResolveOverrides { profile, base_url })
            .context("resolve API credentials")?;
        let client =
            Client::new(&runtime.base_url, &runtime.api_key).context("build API client")?;
        Ok(Self {
            paths,
            profile: runtime.profile,
            client,
        })
    }

    /// Resolve the workspace to act on, honoring `workspace use`.
    pub fn workspace(&self, flag: Option<String>) -> Result<String> {
        require_workspace(&self.paths, &self.profile, flag)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn some(id: &str) -> Option<String> {
        Some(id.to_string())
    }

    #[test]
    fn each_scope_resolves_alone() {
        assert_eq!(
            resolve_scope(some("actor-1"), None, None).expect("actor"),
            FactScope::Actor("actor-1".into())
        );
        assert_eq!(
            resolve_scope(None, some("proj-1"), None).expect("project"),
            FactScope::Project("proj-1".into())
        );
        assert_eq!(
            resolve_scope(None, None, some("agent-1")).expect("agent"),
            FactScope::Agent("agent-1".into())
        );
    }

    #[test]
    fn any_two_scopes_are_rejected() {
        for (actor, project, agent) in [
            (some("a"), some("p"), None),
            (some("a"), None, some("g")),
            (None, some("p"), some("g")),
            (some("a"), some("p"), some("g")),
        ] {
            let err = resolve_scope(actor, project, agent).expect_err("several scopes");
            assert!(err.to_string().contains("not several"), "{err}");
        }
    }

    #[test]
    fn a_missing_scope_is_rejected_with_every_option_named() {
        let message = resolve_scope(None, None, None)
            .expect_err("missing scope must be rejected")
            .to_string();
        for flag in ["--actor", "--project", "--agent"] {
            assert!(message.contains(flag), "{message}");
        }
    }
}
