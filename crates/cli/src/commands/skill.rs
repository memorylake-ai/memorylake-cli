//! `memorylake skill` commands.
//!
//! A skill is a ZIP package with a `SKILL.md` at its root (or one directory
//! down). `create` and `version create` upload the archive and publish it in
//! one step. Every published version goes through a security review, and only
//! a version whose review passed (`safe`) can be referenced from an agent's
//! `skills` list — anything else is refused with `SKILL_NOT_USABLE`.

use std::path::{Path, PathBuf};

use anyhow::{Context, Result, bail};
use clap::{Args, Subcommand};
use memorylake_core::Client;
use memorylake_core::api::skills::{
    CreateSkillRequest, CreateSkillVersionRequest, ListSkillVersionsParams, ListSkillsParams,
    PackageRef, UpdateSkillRequest, create_skill, create_skill_version, delete_skill, get_skill,
    get_skill_version, list_skill_versions, list_skills, update_skill, upload_package,
    validate_package,
};

use super::{api_client, parse_non_blank, print_json};

/// Longest skill name the API accepts.
const MAX_NAME_CHARS: usize = 255;
/// Longest skill title the API accepts.
const MAX_TITLE_CHARS: usize = 500;

/// Skill subcommands.
#[derive(Debug, Subcommand)]
pub enum SkillCommand {
    /// List your skills, newest first.
    List {
        /// Number of items per page (1-100).
        #[arg(long, value_parser = clap::value_parser!(u32).range(1..=100))]
        page_size: Option<u32>,
        /// Continuation token from a previous response.
        #[arg(long)]
        continuation_token: Option<String>,
        /// Fuzzy filter by skill name (partial match).
        #[arg(long = "name")]
        name_fuzzy: Option<String>,
    },
    /// Publish a new skill from a ZIP package.
    ///
    /// The archive must hold a SKILL.md at its root or one directory down and
    /// be at most 10 MiB. The skill comes back with its security review
    /// `pending`; poll `skill get <id>` until `latest_security_status` is
    /// `safe` before referencing it from an agent's `skills` list
    /// (`[{"skill_id": ..., "skill_version": N}]`). A skill that is still
    /// pending, blocked or errored is refused there with `SKILL_NOT_USABLE`.
    ///
    /// Pass a .zip, not a directory: `cd my-skill && zip -r ../my-skill.zip .`
    Create {
        /// Stable identifier for the skill. Must be unique in the team.
        #[arg(long, value_parser = parse_name)]
        name: String,
        /// Title shown in the console.
        #[arg(long, value_parser = parse_title)]
        title: String,
        /// What the skill does.
        #[arg(long)]
        description: Option<String>,
        #[command(flatten)]
        package: PackageArgs,
    },
    /// Get a skill, including the review state of its latest version.
    Get {
        /// Skill id.
        #[arg(value_parser = parse_non_blank)]
        id: String,
    },
    /// Change a skill's title or description.
    ///
    /// Only the flags you pass are sent. To replace the package, publish a new
    /// version with `skill version create`.
    Update {
        /// Skill id.
        #[arg(value_parser = parse_non_blank)]
        id: String,
        /// New title shown in the console.
        #[arg(long, value_parser = parse_title)]
        title: Option<String>,
        /// New description. Pass "" to clear it.
        #[arg(long)]
        description: Option<String>,
    },
    /// Delete a skill and every one of its versions.
    ///
    /// Agent versions that already reference it are not rewritten. Built-in
    /// skills cannot be deleted. There is no confirmation prompt.
    Delete {
        /// Skill id.
        #[arg(value_parser = parse_non_blank)]
        id: String,
    },
    /// Upload a ZIP package without publishing it, and print its storage URI.
    ///
    /// Pass the printed URI to `--package-uri` to create a skill or publish a
    /// version from it without uploading again.
    Upload {
        /// ZIP archive to upload.
        #[arg(value_name = "ZIP")]
        path: PathBuf,
    },
    /// Manage a skill's published versions.
    Version {
        #[command(subcommand)]
        command: VersionCommand,
    },
}

/// `skill version` subcommands.
#[derive(Debug, Subcommand)]
pub enum VersionCommand {
    /// Publish a new version of a skill from a ZIP package.
    ///
    /// The new version must pass security review, so agents that follow the
    /// skill's latest version cannot use it until then; agents pinned to an
    /// older version number are unaffected.
    Create {
        /// Skill id.
        #[arg(value_parser = parse_non_blank)]
        id: String,
        /// Release notes for this version.
        #[arg(long)]
        changelog: Option<String>,
        #[command(flatten)]
        package: PackageArgs,
    },
    /// List a skill's versions, newest first.
    List {
        /// Skill id.
        #[arg(value_parser = parse_non_blank)]
        id: String,
        /// Number of items per page (1-100).
        #[arg(long, value_parser = clap::value_parser!(u32).range(1..=100))]
        page_size: Option<u32>,
        /// Continuation token from a previous response.
        #[arg(long)]
        continuation_token: Option<String>,
    },
    /// Get one version of a skill, including its review state.
    Get {
        /// Skill id.
        #[arg(value_parser = parse_non_blank)]
        id: String,
        /// Version number.
        version: u64,
    },
}

/// Where the package to publish comes from: exactly one of the two.
#[derive(Debug, Clone, Args)]
#[group(required = true, multiple = false)]
pub struct PackageArgs {
    /// ZIP archive to upload and publish (at most 10 MiB).
    #[arg(long, value_name = "ZIP")]
    package: Option<PathBuf>,
    /// Storage URI printed by an earlier `skill upload`.
    #[arg(long, value_name = "URI", value_parser = parse_non_blank)]
    package_uri: Option<String>,
}

impl PackageArgs {
    /// Validate a local archive before anything touches the network.
    fn check(&self) -> Result<()> {
        match &self.package {
            Some(path) => validate_package(path).map(|_| ()).map_err(Into::into),
            None => Ok(()),
        }
    }

    /// Upload the archive if one was given, and return the reference to it.
    fn resolve(self, client: &Client) -> Result<ResolvedPackage> {
        let (s3_uri, uploaded) = match (self.package, self.package_uri) {
            (Some(path), _) => (upload(client, &path)?, true),
            (None, Some(uri)) => (uri, false),
            (None, None) => bail!("pass --package <ZIP> or --package-uri <URI>"),
        };
        Ok(ResolvedPackage {
            package_ref: PackageRef { s3_uri },
            uploaded,
        })
    }
}

/// A package reference, and whether this run uploaded it.
struct ResolvedPackage {
    package_ref: PackageRef,
    uploaded: bool,
}

impl ResolvedPackage {
    /// Error context for the publish call that follows the upload.
    ///
    /// When this run uploaded the archive, a failure after that point (a name
    /// conflict, a package the server rejects for a fixable reason elsewhere)
    /// should not cost a second upload: name the URI and how to reuse it.
    fn publish_context(&self, action: String) -> String {
        if !self.uploaded {
            return action;
        }
        let uri = &self.package_ref.s3_uri;
        format!(
            "{action}\nThe package was uploaded as {uri}; to retry without uploading again, \
             replace --package with: --package-uri {uri}"
        )
    }
}

/// Execute a `skill` subcommand.
pub fn run(command: SkillCommand, profile: Option<String>, base_url: Option<String>) -> Result<()> {
    check_locally(&command)?;
    let client = api_client(profile, base_url)?;

    match command {
        SkillCommand::List {
            page_size,
            continuation_token,
            name_fuzzy,
        } => {
            let data = list_skills(
                &client,
                &ListSkillsParams {
                    page_size,
                    continuation_token,
                    name_fuzzy,
                },
            )
            .context("list skills")?;
            print_json(&data)?;
        }
        SkillCommand::Create {
            name,
            title,
            description,
            package,
        } => {
            let package = package.resolve(&client)?;
            let data = create_skill(
                &client,
                &CreateSkillRequest {
                    name: name.clone(),
                    display_title: title,
                    description,
                    package_ref: package.package_ref.clone(),
                },
            )
            .with_context(|| package.publish_context(format!("create skill `{name}`")))?;
            print_json(&data)?;
            hint_if_pending(&data.id, data.latest_security_status.as_deref());
        }
        SkillCommand::Get { id } => {
            let data = get_skill(&client, &id).with_context(|| format!("get skill `{id}`"))?;
            print_json(&data)?;
        }
        SkillCommand::Update {
            id,
            title,
            description,
        } => {
            let data = update_skill(
                &client,
                &id,
                &UpdateSkillRequest {
                    display_title: title,
                    description,
                },
            )
            .with_context(|| format!("update skill `{id}`"))?;
            print_json(&data)?;
        }
        SkillCommand::Delete { id } => {
            delete_skill(&client, &id).with_context(|| format!("delete skill `{id}`"))?;
            println!("Deleted skill `{id}` and all of its versions");
        }
        SkillCommand::Upload { path } => {
            let s3_uri = upload(&client, &path)?;
            print_json(&serde_json::json!({ "s3_uri": s3_uri }))?;
        }
        SkillCommand::Version { command } => run_version(&client, command)?,
    }

    Ok(())
}

fn run_version(client: &Client, command: VersionCommand) -> Result<()> {
    match command {
        VersionCommand::Create {
            id,
            changelog,
            package,
        } => {
            let package = package.resolve(client)?;
            let data = create_skill_version(
                client,
                &id,
                &CreateSkillVersionRequest {
                    changelog,
                    package_ref: package.package_ref.clone(),
                },
            )
            .with_context(|| {
                package.publish_context(format!("publish a new version of skill `{id}`"))
            })?;
            print_json(&data)?;
            hint_if_pending(&id, data.security_status.as_deref());
        }
        VersionCommand::List {
            id,
            page_size,
            continuation_token,
        } => {
            let data = list_skill_versions(
                client,
                &id,
                &ListSkillVersionsParams {
                    page_size,
                    continuation_token,
                },
            )
            .with_context(|| format!("list versions of skill `{id}`"))?;
            print_json(&data)?;
        }
        VersionCommand::Get { id, version } => {
            let data = get_skill_version(client, &id, version)
                .with_context(|| format!("get version {version} of skill `{id}`"))?;
            print_json(&data)?;
        }
    }

    Ok(())
}

/// Reject what can be judged without credentials or the network.
fn check_locally(command: &SkillCommand) -> Result<()> {
    match command {
        SkillCommand::Create { package, .. } => package.check(),
        SkillCommand::Version {
            command: VersionCommand::Create { package, .. },
        } => package.check(),
        SkillCommand::Upload { path } => validate_package(path).map(|_| ()).map_err(Into::into),
        SkillCommand::Update {
            title: None,
            description: None,
            ..
        } => bail!("nothing to update; pass --title and/or --description"),
        _ => Ok(()),
    }
}

fn upload(client: &Client, path: &Path) -> Result<String> {
    upload_package(client, path).with_context(|| format!("upload skill package {}", path.display()))
}

/// Tell the caller how to follow a review that has not finished yet.
///
/// Goes to stderr so stdout stays a single JSON document.
fn hint_if_pending(id: &str, status: Option<&str>) {
    if status == Some("pending") {
        eprintln!(
            "Security review pending; agents cannot use this version until it is `safe`.\n\
             Check with: memorylake skill get {id}"
        );
    }
}

fn parse_name(raw: &str) -> std::result::Result<String, String> {
    parse_bounded(raw, MAX_NAME_CHARS)
}

fn parse_title(raw: &str) -> std::result::Result<String, String> {
    parse_bounded(raw, MAX_TITLE_CHARS)
}

/// Accept a non-blank value of at most `max` characters, unchanged.
fn parse_bounded(raw: &str, max: usize) -> std::result::Result<String, String> {
    if raw.trim().is_empty() {
        return Err("must not be empty".to_string());
    }
    let chars = raw.chars().count();
    if chars > max {
        return Err(format!("must be at most {max} characters, got {chars}"));
    }
    Ok(raw.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn bounded_values_reject_blank_and_overlong_input() {
        assert!(parse_name("  ").is_err());
        assert!(parse_name(&"x".repeat(MAX_NAME_CHARS)).is_ok());
        let err = parse_name(&"x".repeat(MAX_NAME_CHARS + 1)).unwrap_err();
        assert!(err.contains("at most 255"), "{err}");
        assert!(parse_title(&"é".repeat(MAX_TITLE_CHARS)).is_ok());
    }

    #[test]
    fn bounded_values_are_kept_verbatim() {
        assert_eq!(
            parse_title(" Equity Research ").unwrap(),
            " Equity Research "
        );
    }
}
