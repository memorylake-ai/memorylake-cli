//! `memorylake agent-skills` commands.
//!
//! Installs the skills bundled with this binary (the repository's `skills/`)
//! into the directories coding agents read skills from, keeps them current
//! across upgrades, and removes them again. A skill directory this command did
//! not write, or one edited since it was written, is never replaced or removed
//! without `--force` — and one it did not write, not even then.

use std::collections::BTreeMap;
use std::fs;
use std::path::{Path, PathBuf};

use anyhow::{Context, Result, bail};
use clap::{Args, Subcommand, ValueEnum};
use serde::{Deserialize, Serialize};

use super::print_json;

include!(concat!(env!("OUT_DIR"), "/agent_skills_bundle.rs"));

/// Environment variable naming the home directory whose `.claude` and
/// `.agents` receive skills, in place of the user's own.
///
/// It exists for the reason `MEMORYLAKE_CONFIG_DIR` does: on Windows the home
/// directory cannot be redirected through `HOME` or `USERPROFILE`, so a test
/// or a container would otherwise write into the real user's agents.
const ENV_HOME: &str = "MEMORYLAKE_AGENT_SKILLS_HOME";

/// Written into every installed skill directory: which skill, from which
/// release, and a digest of exactly what was written. A directory without one
/// was not installed here; one whose files no longer match the digest has been
/// edited since.
const MARKER_FILE: &str = ".memorylake-skill.toml";

/// Files an operating system leaves behind, which neither count as an edit nor
/// as part of a skill. Kept in step with `build.rs`.
const IGNORED_NAMES: &[&str] = &[".DS_Store", "Thumbs.db"];

/// Agent-skills subcommands.
#[derive(Debug, Subcommand)]
pub enum AgentSkillsCommand {
    /// List the skills bundled with this binary and where each target
    /// directory is.
    List,
    /// Install the bundled skills, or bring installed ones up to date.
    ///
    /// Without `--target`, installs into every target whose agent is found on
    /// this machine. A skill directory that was not installed by this command
    /// is left alone, and one edited since it was installed is kept unless
    /// `--force` is passed. Skills a newer binary no longer bundles are
    /// removed.
    Install {
        #[command(flatten)]
        targets: TargetArgs,
        /// Replace installed skills even if they were edited since.
        #[arg(long)]
        force: bool,
    },
    /// Show, per target, whether each bundled skill is installed and current.
    Status {
        #[command(flatten)]
        targets: TargetArgs,
    },
    /// Remove the skills this command installed.
    ///
    /// Without `--target`, removes them from every target. A skill edited
    /// since it was installed is kept unless `--force` is passed.
    Uninstall {
        #[command(flatten)]
        targets: TargetArgs,
        /// Remove installed skills even if they were edited since.
        #[arg(long)]
        force: bool,
    },
}

/// Which skill directories a command acts on.
#[derive(Debug, Args)]
pub struct TargetArgs {
    /// `claude` (`.claude/skills`: Claude Code, opencode), `agents`
    /// (`.agents/skills`: Codex, Gemini CLI, opencode, and other agents that
    /// follow the Agent Skills standard), or `all`. Repeatable.
    #[arg(long = "target", value_enum)]
    targets: Vec<TargetArg>,
    /// Use the current directory's `.claude/skills` and `.agents/skills`
    /// instead of the home directory's, for skills committed with a
    /// repository.
    #[arg(long)]
    project: bool,
}

/// What `--target` accepts.
#[derive(Debug, Clone, Copy, PartialEq, Eq, ValueEnum)]
enum TargetArg {
    Claude,
    Agents,
    All,
}

/// A directory coding agents read skills from.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize)]
#[serde(rename_all = "lowercase")]
enum Target {
    Claude,
    Agents,
}

impl Target {
    const ALL: [Self; 2] = [Self::Claude, Self::Agents];

    /// The skills directory under `base` (a home or a project directory).
    fn dir(self, base: &Path) -> PathBuf {
        let parent = match self {
            Self::Claude => ".claude",
            Self::Agents => ".agents",
        };
        base.join(parent).join("skills")
    }

    /// The agents known to read this directory.
    fn read_by(self) -> &'static [&'static str] {
        match self {
            Self::Claude => &["Claude Code", "opencode"],
            Self::Agents => &["Codex", "Gemini CLI", "opencode"],
        }
    }

    /// Whether an agent that reads this directory is set up under `home`,
    /// judged by the configuration directories those agents create.
    fn detected(self, home: &Path) -> bool {
        let markers: &[&str] = match self {
            Self::Claude => &[".claude"],
            Self::Agents => &[".agents", ".codex", ".gemini", ".config/opencode"],
        };
        markers.iter().any(|marker| home.join(marker).is_dir())
    }
}

/// One bundled skill: its directory name and its files, relative to it.
struct Skill {
    name: &'static str,
    files: Vec<(&'static str, &'static [u8])>,
}

impl Skill {
    fn digest(&self) -> String {
        digest(self.files.iter().map(|(path, bytes)| (*path, *bytes)))
    }

    /// The `description` from the frontmatter of `SKILL.md`.
    fn description(&self) -> Option<String> {
        let (_, skill_md) = self.files.iter().find(|(path, _)| *path == "SKILL.md")?;
        frontmatter_value(&String::from_utf8_lossy(skill_md), "description")
    }
}

/// The bundled skills, grouped by their top-level directory under `skills/`.
/// Files directly in `skills/` belong to no skill and are not installed.
fn bundled_skills() -> Vec<Skill> {
    let mut by_name: BTreeMap<&'static str, Vec<(&'static str, &'static [u8])>> = BTreeMap::new();
    for (path, bytes) in BUNDLED_FILES {
        if let Some((name, relative)) = path.split_once('/') {
            by_name.entry(name).or_default().push((relative, *bytes));
        }
    }
    by_name
        .into_iter()
        .map(|(name, files)| Skill { name, files })
        .collect()
}

/// Read one single-line `key: value` from a Markdown file's YAML frontmatter.
fn frontmatter_value(markdown: &str, key: &str) -> Option<String> {
    let mut lines = markdown.lines();
    if lines.next()?.trim_end() != "---" {
        return None;
    }
    lines
        .take_while(|line| line.trim_end() != "---")
        .find_map(|line| line.strip_prefix(key)?.strip_prefix(':'))
        .map(|value| value.trim().to_string())
}

/// FNV-1a over each file's path, length and contents, in the order given.
///
/// This detects edits, not tampering, so a cryptographic hash would buy
/// nothing; the length prefix keeps one file's bytes from passing for another
/// file's path.
fn digest<'a>(files: impl IntoIterator<Item = (&'a str, &'a [u8])>) -> String {
    const OFFSET: u64 = 0xcbf2_9ce4_8422_2325;
    const PRIME: u64 = 0x0000_0100_0000_01b3;
    let mut hash = OFFSET;
    let mut feed = |bytes: &[u8]| {
        for byte in bytes {
            hash ^= u64::from(*byte);
            hash = hash.wrapping_mul(PRIME);
        }
    };
    for (path, bytes) in files {
        feed(path.as_bytes());
        feed(&[0]);
        feed(&(bytes.len() as u64).to_le_bytes());
        feed(bytes);
    }
    format!("{hash:016x}")
}

/// What [`MARKER_FILE`] records.
#[derive(Debug, Serialize, Deserialize)]
struct Marker {
    skill: String,
    version: String,
    digest: String,
}

/// How an installed skill directory compares with what was written there and
/// with what this binary bundles.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
enum State {
    /// Nothing at that path.
    Missing,
    /// Something this command did not write.
    Foreign,
    /// Written by this command and unchanged, and it matches the bundle.
    Current,
    /// Written by this command and unchanged, but from another release.
    Outdated,
    /// Written by this command, then edited.
    Modified,
}

/// Inspect the skill directory at `dir` against the bundled digest, if the
/// skill is still bundled.
fn inspect(dir: &Path, bundled_digest: Option<&str>) -> Result<(State, Option<Marker>)> {
    if !dir.exists() {
        return Ok((State::Missing, None));
    }
    let Some(marker) = read_marker(dir) else {
        return Ok((State::Foreign, None));
    };
    let on_disk = read_tree(dir)?;
    let on_disk_digest = digest(
        on_disk
            .iter()
            .map(|(path, bytes)| (path.as_str(), bytes.as_slice())),
    );
    let state = if on_disk_digest != marker.digest {
        State::Modified
    } else if bundled_digest == Some(marker.digest.as_str()) {
        State::Current
    } else {
        State::Outdated
    };
    Ok((state, Some(marker)))
}

fn read_marker(dir: &Path) -> Option<Marker> {
    let text = fs::read_to_string(dir.join(MARKER_FILE)).ok()?;
    toml::from_str(&text).ok()
}

/// Every file under `dir` except the marker, as (path relative to `dir` with
/// `/` separators, contents), sorted by path — the order [`digest`] needs.
fn read_tree(dir: &Path) -> Result<Vec<(String, Vec<u8>)>> {
    fn walk(root: &Path, dir: &Path, files: &mut Vec<(String, Vec<u8>)>) -> Result<()> {
        for entry in fs::read_dir(dir).with_context(|| format!("read {}", dir.display()))? {
            let entry = entry.with_context(|| format!("read {}", dir.display()))?;
            let name = entry.file_name();
            if IGNORED_NAMES.iter().any(|ignored| name == *ignored) {
                continue;
            }
            let path = entry.path();
            if path.is_dir() {
                walk(root, &path, files)?;
                continue;
            }
            let relative = path
                .strip_prefix(root)
                .expect("walked from root")
                .components()
                .map(|part| part.as_os_str().to_string_lossy())
                .collect::<Vec<_>>()
                .join("/");
            if relative == MARKER_FILE {
                continue;
            }
            let bytes = fs::read(&path).with_context(|| format!("read {}", path.display()))?;
            files.push((relative, bytes));
        }
        Ok(())
    }

    let mut files = Vec::new();
    walk(dir, dir, &mut files)?;
    files.sort();
    Ok(files)
}

/// Directories under `root` that this command installed, by name.
fn managed_dirs(root: &Path) -> Result<Vec<String>> {
    if !root.is_dir() {
        return Ok(Vec::new());
    }
    let mut names = Vec::new();
    for entry in fs::read_dir(root).with_context(|| format!("read {}", root.display()))? {
        let entry = entry.with_context(|| format!("read {}", root.display()))?;
        if entry.path().join(MARKER_FILE).is_file() {
            names.push(entry.file_name().to_string_lossy().into_owned());
        }
    }
    names.sort();
    Ok(names)
}

/// Write `skill` to `root/<name>`, replacing whatever is there.
///
/// The files are staged beside `root` rather than inside it, so an
/// interrupted install never leaves a half-written directory where an agent
/// would load it as a skill.
fn write_skill(root: &Path, skill: &Skill, version: &str) -> Result<()> {
    fs::create_dir_all(root).with_context(|| format!("create {}", root.display()))?;
    let staging = root
        .parent()
        .expect("a target directory has a parent")
        .join(format!(".memorylake-skill-staging-{}", skill.name));
    if staging.exists() {
        fs::remove_dir_all(&staging).with_context(|| format!("remove {}", staging.display()))?;
    }

    for (relative, bytes) in &skill.files {
        let path = relative
            .split('/')
            .fold(staging.clone(), |path, part| path.join(part));
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent).with_context(|| format!("create {}", parent.display()))?;
        }
        fs::write(&path, bytes).with_context(|| format!("write {}", path.display()))?;
    }
    let marker = Marker {
        skill: skill.name.to_string(),
        version: version.to_string(),
        digest: skill.digest(),
    };
    let marker_text = format!(
        "# Written by `memorylake agent-skills install`; `memorylake agent-skills uninstall` removes it.\n{}",
        toml::to_string(&marker).context("encode skill marker")?
    );
    let marker_path = staging.join(MARKER_FILE);
    fs::write(&marker_path, marker_text)
        .with_context(|| format!("write {}", marker_path.display()))?;

    let dest = root.join(skill.name);
    if dest.exists() {
        fs::remove_dir_all(&dest).with_context(|| format!("remove {}", dest.display()))?;
    }
    fs::rename(&staging, &dest)
        .with_context(|| format!("move {} to {}", staging.display(), dest.display()))
}

/// What a command did to one skill directory.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
enum Action {
    Installed,
    Updated,
    Unchanged,
    Removed,
    Skipped,
}

#[derive(Debug, Serialize)]
struct Outcome {
    target: Target,
    skill: String,
    path: PathBuf,
    action: Action,
    #[serde(skip_serializing_if = "Option::is_none")]
    reason: Option<&'static str>,
}

impl Outcome {
    /// Say what happened on standard error, for a person watching an install.
    fn announce(&self) {
        let path = self.path.display();
        match (self.action, self.reason) {
            (Action::Skipped, Some(reason)) => eprintln!("skipped {}: {reason}", path),
            (Action::Unchanged, _) => eprintln!("up to date {path}"),
            (action, _) => eprintln!("{} {path}", action.as_str()),
        }
    }
}

impl Action {
    /// The word its JSON uses.
    fn as_str(self) -> &'static str {
        match self {
            Self::Installed => "installed",
            Self::Updated => "updated",
            Self::Unchanged => "unchanged",
            Self::Removed => "removed",
            Self::Skipped => "skipped",
        }
    }
}

const REASON_FOREIGN: &str = "not installed by memorylake, so it is left alone";
const REASON_MODIFIED: &str = "edited since it was installed; --force replaces it";
const REASON_MODIFIED_RETIRED: &str =
    "no longer bundled, but edited since it was installed; --force removes it";
const REASON_MODIFIED_UNINSTALL: &str = "edited since it was installed; --force removes it";

/// Execute an `agent-skills` subcommand.
pub fn run(command: AgentSkillsCommand) -> Result<()> {
    let version = crate::VERSION;
    let skills = bundled_skills();

    match command {
        AgentSkillsCommand::List => {
            let home = home_dir()?;
            let skills: Vec<_> = skills
                .iter()
                .map(|skill| {
                    serde_json::json!({
                        "name": skill.name,
                        "description": skill.description(),
                    })
                })
                .collect();
            let targets: Vec<_> = Target::ALL
                .iter()
                .map(|target| {
                    serde_json::json!({
                        "target": target,
                        "path": target.dir(&home),
                        "detected": target.detected(&home),
                        "read_by": target.read_by(),
                    })
                })
                .collect();
            print_json(&serde_json::json!({
                "version": version,
                "skills": skills,
                "targets": targets,
            }))
        }
        AgentSkillsCommand::Install { targets, force } => {
            let (base, targets) = resolve_targets(&targets, DefaultTargets::Detected)?;
            if targets.is_empty() {
                eprintln!(
                    "no coding agent found under {}; pass --target claude, --target agents or --target all to install anyway",
                    base.display()
                );
            }
            let mut outcomes = Vec::new();
            for target in targets {
                install_into(
                    target,
                    &target.dir(&base),
                    &skills,
                    version,
                    force,
                    &mut outcomes,
                )?;
            }
            report(version, outcomes)
        }
        AgentSkillsCommand::Status { targets } => {
            let (base, targets) = resolve_targets(&targets, DefaultTargets::All)?;
            let mut entries = Vec::new();
            for target in targets {
                let root = target.dir(&base);
                for skill in &skills {
                    let dir = root.join(skill.name);
                    let (state, marker) = inspect(&dir, Some(&skill.digest()))?;
                    entries.push(status_entry(target, skill.name, &dir, state, marker));
                }
                for name in managed_dirs(&root)? {
                    if skills.iter().all(|skill| skill.name != name) {
                        let dir = root.join(&name);
                        let (_, marker) = inspect(&dir, None)?;
                        entries.push(status_entry(target, &name, &dir, "retired", marker));
                    }
                }
            }
            print_json(&serde_json::json!({ "version": version, "skills": entries }))
        }
        AgentSkillsCommand::Uninstall { targets, force } => {
            let (base, targets) = resolve_targets(&targets, DefaultTargets::All)?;
            let mut outcomes = Vec::new();
            for target in targets {
                uninstall_from(target, &target.dir(&base), force, &mut outcomes)?;
            }
            report(version, outcomes)
        }
    }
}

/// Which targets a command uses when no `--target` is given.
#[derive(Debug, Clone, Copy)]
enum DefaultTargets {
    /// Those whose agent is found under the home directory — installing must
    /// not create configuration for agents the user does not have.
    Detected,
    /// Every target — reading or removing what is there is harmless anywhere.
    All,
}

/// The base directory (home, or the current directory for `--project`) and
/// the targets to act on there.
fn resolve_targets(args: &TargetArgs, default: DefaultTargets) -> Result<(PathBuf, Vec<Target>)> {
    let base = if args.project {
        std::env::current_dir().context("resolve the current directory")?
    } else {
        home_dir()?
    };

    let mut targets: Vec<Target> = if args.targets.contains(&TargetArg::All) {
        Target::ALL.to_vec()
    } else if !args.targets.is_empty() {
        args.targets
            .iter()
            .map(|target| match target {
                TargetArg::Claude => Target::Claude,
                TargetArg::Agents => Target::Agents,
                TargetArg::All => unreachable!("handled above"),
            })
            .collect()
    } else if args.project {
        // A repository is not "set up" for an agent the way a home directory
        // is, so asking for the project means both.
        Target::ALL.to_vec()
    } else {
        match default {
            DefaultTargets::All => Target::ALL.to_vec(),
            DefaultTargets::Detected => Target::ALL
                .into_iter()
                .filter(|target| target.detected(&base))
                .collect(),
        }
    };
    targets.sort();
    targets.dedup();
    Ok((base, targets))
}

fn home_dir() -> Result<PathBuf> {
    if let Some(dir) = std::env::var_os(ENV_HOME).filter(|dir| !dir.is_empty()) {
        return Ok(PathBuf::from(dir));
    }
    match dirs::home_dir() {
        Some(home) => Ok(home),
        None => bail!("cannot find the home directory; set {ENV_HOME}"),
    }
}

fn install_into(
    target: Target,
    root: &Path,
    skills: &[Skill],
    version: &str,
    force: bool,
    outcomes: &mut Vec<Outcome>,
) -> Result<()> {
    for skill in skills {
        let dir = root.join(skill.name);
        let (state, _) = inspect(&dir, Some(&skill.digest()))?;
        let (action, reason) = match state {
            State::Missing => (Action::Installed, None),
            State::Outdated => (Action::Updated, None),
            State::Modified if force => (Action::Updated, None),
            State::Current => (Action::Unchanged, None),
            State::Modified => (Action::Skipped, Some(REASON_MODIFIED)),
            State::Foreign => (Action::Skipped, Some(REASON_FOREIGN)),
        };
        if matches!(action, Action::Installed | Action::Updated) {
            write_skill(root, skill, version)?;
        }
        push(outcomes, target, skill.name, dir, action, reason);
    }

    // Skills an earlier release installed that this one no longer bundles.
    for name in managed_dirs(root)? {
        if skills.iter().any(|skill| skill.name == name) {
            continue;
        }
        let dir = root.join(&name);
        let (state, _) = inspect(&dir, None)?;
        if state == State::Modified && !force {
            push(
                outcomes,
                target,
                &name,
                dir,
                Action::Skipped,
                Some(REASON_MODIFIED_RETIRED),
            );
        } else {
            fs::remove_dir_all(&dir).with_context(|| format!("remove {}", dir.display()))?;
            push(outcomes, target, &name, dir, Action::Removed, None);
        }
    }
    Ok(())
}

fn uninstall_from(
    target: Target,
    root: &Path,
    force: bool,
    outcomes: &mut Vec<Outcome>,
) -> Result<()> {
    for name in managed_dirs(root)? {
        let dir = root.join(&name);
        let (state, _) = inspect(&dir, None)?;
        if state == State::Modified && !force {
            push(
                outcomes,
                target,
                &name,
                dir,
                Action::Skipped,
                Some(REASON_MODIFIED_UNINSTALL),
            );
        } else {
            fs::remove_dir_all(&dir).with_context(|| format!("remove {}", dir.display()))?;
            push(outcomes, target, &name, dir, Action::Removed, None);
        }
    }
    Ok(())
}

fn push(
    outcomes: &mut Vec<Outcome>,
    target: Target,
    skill: &str,
    path: PathBuf,
    action: Action,
    reason: Option<&'static str>,
) {
    let outcome = Outcome {
        target,
        skill: skill.to_string(),
        path,
        action,
        reason,
    };
    outcome.announce();
    outcomes.push(outcome);
}

fn report(version: &str, outcomes: Vec<Outcome>) -> Result<()> {
    print_json(&serde_json::json!({ "version": version, "results": outcomes }))
}

fn status_entry(
    target: Target,
    skill: &str,
    dir: &Path,
    state: impl Serialize,
    marker: Option<Marker>,
) -> serde_json::Value {
    serde_json::json!({
        "target": target,
        "skill": skill,
        "path": dir,
        "state": state,
        "installed_version": marker.map(|marker| marker.version),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_bundled_skill_has_valid_frontmatter() {
        // Agents refuse or silently drop a skill whose frontmatter `name` does
        // not match its directory, or whose description is missing or too
        // long (opencode: 1-1024 characters; Claude Code truncates).
        let skills = bundled_skills();
        assert!(!skills.is_empty());
        for skill in &skills {
            let (_, skill_md) = skill
                .files
                .iter()
                .find(|(path, _)| *path == "SKILL.md")
                .unwrap_or_else(|| panic!("{} has no SKILL.md", skill.name));
            let text = String::from_utf8_lossy(skill_md);
            assert_eq!(
                frontmatter_value(&text, "name").as_deref(),
                Some(skill.name),
                "{}: frontmatter name must match the directory",
                skill.name
            );
            assert!(
                skill.name.split('-').all(|part| !part.is_empty()
                    && part
                        .chars()
                        .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit())),
                "{}: names are lowercase letters and digits joined by single hyphens",
                skill.name
            );
            let description = skill.description().unwrap_or_default();
            assert!(
                (1..=1024).contains(&description.chars().count()),
                "{}: description must be 1-1024 characters",
                skill.name
            );
        }
    }

    #[test]
    fn every_command_in_the_skills_parses() {
        // A skill that teaches a renamed command or flag sends agents into
        // errors they then try to work around. Every `memorylake` line in a
        // ```bash block goes through the real argument parser, so a CLI change
        // fails here until the skills follow it.
        use clap::Parser;
        use clap::error::ErrorKind;

        let commands = bash_commands();
        assert!(
            commands.len() > 20,
            "expected the skills to carry examples, found {}",
            commands.len()
        );
        let failures: Vec<String> = commands
            .iter()
            .filter_map(|(location, line)| {
                let args = shell_words(line);
                match crate::Cli::try_parse_from(&args) {
                    Ok(_) => None,
                    Err(err) if err.kind() == ErrorKind::DisplayHelp => None,
                    Err(err) => Some(format!("{location}: {line}\n{err}")),
                }
            })
            .collect();
        assert!(failures.is_empty(), "{}", failures.join("\n"));
    }

    #[test]
    fn shell_words_follows_quotes_and_drops_shell_syntax() {
        assert_eq!(
            shell_words(r#"memorylake fact add --actor "$ACTOR" "a b" 'c "d"' | jq .id"#),
            [
                "memorylake",
                "fact",
                "add",
                "--actor",
                "x",
                "a b",
                "c \"d\""
            ]
        );
        assert_eq!(
            shell_words("memorylake actor me   # who am I"),
            ["memorylake", "actor", "me"]
        );
        assert_eq!(
            shell_words("memorylake fact list --query a#b"),
            ["memorylake", "fact", "list", "--query", "a#b"]
        );
        assert_eq!(
            shell_words("memorylake member scope get usr-1 > scope.json 2> etag.txt"),
            ["memorylake", "member", "scope", "get", "usr-1"]
        );
        assert_eq!(
            shell_words("memorylake ag get grp-1 2>etag.txt 2>&1"),
            ["memorylake", "ag", "get", "grp-1"]
        );
    }

    /// Every line starting with `memorylake` inside a ```bash block of a
    /// bundled Markdown file, with `\` continuations joined, as
    /// (`file:line`, command).
    fn bash_commands() -> Vec<(String, String)> {
        let mut commands = Vec::new();
        for (path, bytes) in BUNDLED_FILES {
            if !path.ends_with(".md") {
                continue;
            }
            let text = String::from_utf8_lossy(bytes);
            let mut in_fence = false;
            let mut is_bash = false;
            let mut pending: Option<(usize, String)> = None;
            for (index, line) in text.lines().enumerate() {
                let trimmed = line.trim();
                if trimmed.starts_with("```") {
                    is_bash = !in_fence && trimmed == "```bash";
                    in_fence = !in_fence;
                    continue;
                }
                if !(in_fence && is_bash) {
                    continue;
                }
                let (start, mut command) = match pending.take() {
                    Some((start, joined)) => (start, joined + " " + trimmed),
                    None if trimmed.starts_with("memorylake ") => (index + 1, trimmed.to_string()),
                    None => continue,
                };
                if let Some(stripped) = command.strip_suffix('\\') {
                    command = stripped.trim_end().to_string();
                    pending = Some((start, command));
                } else {
                    commands.push((format!("skills/{path}:{start}"), command));
                }
            }
        }
        commands
    }

    /// Split a shell command line into the command's arguments: single and
    /// double quotes group, an unquoted `#` starting a word begins a comment,
    /// a lone `|` ends the command, redirections are dropped, and a word
    /// starting with `$` (a shell variable or substitution) stands in as `x`.
    fn shell_words(line: &str) -> Vec<String> {
        let mut words = Vec::new();
        let mut word = String::new();
        let mut in_word = false;
        let mut quote: Option<char> = None;
        let mut chars = line.chars().peekable();
        while let Some(c) = chars.next() {
            match quote {
                Some(q) if c == q => quote = None,
                Some(_) => word.push(c),
                None => match c {
                    '\'' | '"' => {
                        quote = Some(c);
                        in_word = true;
                    }
                    '#' if !in_word => break,
                    '|' if !in_word && chars.peek().is_none_or(|next| next.is_whitespace()) => {
                        break;
                    }
                    c if c.is_whitespace() => {
                        if in_word {
                            words.push(std::mem::take(&mut word));
                            in_word = false;
                        }
                    }
                    c => {
                        word.push(c);
                        in_word = true;
                    }
                },
            }
        }
        if in_word {
            words.push(word);
        }

        let mut args = Vec::new();
        let mut words = words.into_iter();
        while let Some(word) = words.next() {
            // A redirection (`> out.json`, `2> etag.txt`, `2>&1`) is the
            // shell's, not the command's: drop it, with its target when that
            // is the next word.
            let operator = word.trim_start_matches(|c: char| c.is_ascii_digit() || c == '&');
            if operator.starts_with(['>', '<']) {
                if operator.trim_start_matches(['>', '<', '&']).is_empty() {
                    words.next();
                }
                continue;
            }
            args.push(if word.starts_with('$') {
                "x".to_string()
            } else {
                word
            });
        }
        args
    }

    #[test]
    fn frontmatter_value_reads_only_the_frontmatter() {
        let text = "---\nname: a\ndescription: says: this\n---\ndescription: body\n";
        assert_eq!(frontmatter_value(text, "name").as_deref(), Some("a"));
        assert_eq!(
            frontmatter_value(text, "description").as_deref(),
            Some("says: this")
        );
        assert_eq!(frontmatter_value("no frontmatter", "name"), None);
        assert_eq!(
            frontmatter_value("---\nname: a\n---\nmissing: b\n", "missing"),
            None
        );
    }

    #[test]
    fn inspect_tells_current_outdated_and_modified_apart() {
        let base = std::env::temp_dir().join(format!(
            "mlcli-agent-skills-{}-{:?}",
            std::process::id(),
            std::thread::current().id()
        ));
        let root = base.join("skills");
        let skill = Skill {
            name: "demo",
            files: vec![("SKILL.md", b"one"), ("references/a.md", b"a")],
        };
        write_skill(&root, &skill, "v1").expect("write");
        let dir = root.join("demo");
        assert_eq!(
            inspect(&dir, Some(&skill.digest())).expect("inspect").0,
            State::Current
        );
        // Unchanged since it was written, but a newer binary bundles another digest.
        assert_eq!(
            inspect(&dir, Some("0")).expect("inspect").0,
            State::Outdated
        );
        fs::write(dir.join("references").join("a.md"), "b").expect("edit");
        assert_eq!(
            inspect(&dir, Some(&skill.digest())).expect("inspect").0,
            State::Modified
        );
        // The staging directory is gone once the skill is in place.
        let leftovers: Vec<_> = fs::read_dir(&base)
            .expect("read")
            .map(|entry| entry.expect("entry").file_name())
            .collect();
        assert_eq!(leftovers, ["skills"]);
        fs::remove_dir_all(&base).expect("clean up");
    }

    #[test]
    fn digest_separates_paths_from_contents() {
        let one = digest([("ab", b"c".as_slice())]);
        let other = digest([("a", b"bc".as_slice())]);
        assert_ne!(one, other);
        assert_eq!(one, digest([("ab", b"c".as_slice())]));
    }
}
