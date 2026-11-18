//! `memorylake self` commands: manage this installation of the CLI.
//!
//! `self update` replaces the running binary with a published release: the
//! same archive the installers download, verified against the same `.sha256`
//! published beside it. Credentials and configuration live elsewhere and are
//! never touched.

use std::fs;
use std::io::{Cursor, Read};
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::time::Duration;

use anyhow::{Context, Result, bail};
use clap::{Args, Subcommand};
use reqwest::blocking::Client;
use reqwest::header::LOCATION;
use reqwest::redirect::Policy;
use serde::Serialize;

use super::{agent_skills, print_json};

/// Where releases are published. `<url>/latest` redirects to the newest tag;
/// `<url>/download/<tag>/<asset>` serves its archives.
const RELEASES_URL: &str = "https://github.com/memorylake-ai/memorylake-cli/releases";

/// Environment variable naming another releases URL laid out the same way,
/// for a mirror or a test server.
const ENV_RELEASES_URL: &str = "MEMORYLAKE_RELEASES_URL";

/// The target triple this binary was built for, which names its archive.
const TARGET: &str = env!("MEMORYLAKE_TARGET");

/// Refuse anything larger. A release archive is a few megabytes; this only
/// stops a wrong URL from filling memory.
const MAX_DOWNLOAD_BYTES: u64 = 256 * 1024 * 1024;

/// `self` subcommands.
#[derive(Debug, Subcommand)]
pub enum SelfCommand {
    /// Replace this binary with the latest release, or the one `--version`
    /// names.
    ///
    /// Downloads the release archive for this platform, verifies it against
    /// its published SHA-256, checks that the new binary runs and reports the
    /// expected release, and only then swaps it in. Credentials and
    /// configuration are left alone. Skills that `agent-skills install` put
    /// into coding agents are refreshed from the new binary.
    Update(UpdateArgs),
}

/// Arguments to `self update`.
#[derive(Debug, Args)]
pub struct UpdateArgs {
    /// Release tag to install instead of the latest, e.g. `v20261117`. An
    /// older tag goes back to that release.
    #[arg(long, value_parser = parse_tag)]
    version: Option<String>,
    /// Only report whether an update is available; change nothing.
    #[arg(long, conflicts_with = "force")]
    check: bool,
    /// Reinstall even if that release is already installed, or replace a
    /// build that did not come from a release (a dev build).
    #[arg(long)]
    force: bool,
    /// Leave the skills installed into coding agents as they are.
    #[arg(long)]
    no_skills: bool,
}

/// What `self update` prints.
#[derive(Debug, Serialize)]
struct UpdateReport {
    /// The version that was running.
    from: String,
    /// The release asked for: the latest, or `--version`.
    to: String,
    /// Whether `to` would replace `from`. Never true for an older release
    /// unless `--version` asked for it.
    update_available: bool,
    /// Whether the binary was replaced.
    updated: bool,
    /// The binary that was (or would be) replaced.
    path: PathBuf,
    /// What refreshing the coding-agent skills did, when that ran.
    #[serde(skip_serializing_if = "Option::is_none")]
    agent_skills: Option<serde_json::Value>,
}

/// What an update works on: the binary installed, which build it is, and
/// where releases come from. [`update`] describes this process; tests
/// describe a copy served releases locally.
struct Installation {
    /// What `version` reports.
    running: String,
    /// The release tag the binary was built as; `None` for a dev build.
    release: Option<String>,
    /// The binary to replace.
    exe: PathBuf,
    /// The releases URL, without a trailing slash.
    releases: String,
}

/// Execute a `self` subcommand.
pub fn run(command: SelfCommand) -> Result<()> {
    match command {
        SelfCommand::Update(args) => {
            let installation = Installation {
                running: crate::VERSION.to_string(),
                release: crate::RELEASE.map(str::to_string),
                exe: current_exe()?,
                releases: releases_url(),
            };
            print_json(&update(&args, &installation)?)
        }
    }
}

fn update(args: &UpdateArgs, installation: &Installation) -> Result<UpdateReport> {
    let Installation {
        running,
        release,
        exe,
        releases,
    } = installation;

    // Decided before going to the network, so a dev build is turned away
    // without waiting on GitHub.
    if release.is_none() && !args.check && !args.force {
        bail!(
            "this is a dev build ({running}), not a release, so `self update` will not \
             replace it\npass --force to replace it with a release anyway"
        );
    }

    let tag = match &args.version {
        Some(tag) => tag.clone(),
        None => {
            eprintln!("checking for the latest release");
            latest_tag(releases)?
        }
    };
    let update_available = match release {
        None => true,
        // An explicit tag is wanted whatever its age; the latest only when
        // it is newer, so a build tagged after the last published release is
        // not "updated" backwards.
        Some(current) if args.version.is_some() => *current != tag,
        Some(current) => is_newer(&tag, current),
    };
    let mut report = UpdateReport {
        from: running.clone(),
        to: tag.clone(),
        update_available,
        updated: false,
        path: exe.clone(),
        agent_skills: None,
    };

    if args.check {
        if update_available {
            eprintln!("{tag} is available (installed: {running})");
        } else {
            eprintln!("up to date ({running}; latest release: {tag})");
        }
        return Ok(report);
    }
    if !update_available && !args.force {
        eprintln!("already up to date ({running}); --force reinstalls {tag}");
        return Ok(report);
    }

    let binary = download_release(releases, &tag)?;
    let staged = Staged::write(exe, &binary)?;
    check_runs(&staged.0, &tag)?;
    staged.swap_into(exe)?;
    report.updated = true;
    eprintln!("updated {}: {running} -> {tag}", exe.display());

    if !args.no_skills {
        report.agent_skills = refresh_agent_skills(exe);
    }
    Ok(report)
}

/// The releases URL, without a trailing slash.
fn releases_url() -> String {
    let url = std::env::var(ENV_RELEASES_URL)
        .ok()
        .filter(|url| !url.trim().is_empty())
        .unwrap_or_else(|| RELEASES_URL.to_string());
    url.trim().trim_end_matches('/').to_string()
}

/// Accept a release tag, adding the `v` that a bare `20261117` lacks.
///
/// Only characters a tag can carry are allowed, since it becomes part of a
/// download URL.
fn parse_tag(raw: &str) -> std::result::Result<String, String> {
    let tag = raw.trim();
    if tag.is_empty() {
        return Err("must not be empty".to_string());
    }
    if !tag
        .chars()
        .all(|c| c.is_ascii_alphanumeric() || matches!(c, '.' | '-' | '_'))
    {
        return Err(format!("`{tag}` is not a release tag, e.g. v20261117"));
    }
    Ok(if tag.starts_with(|c: char| c.is_ascii_digit()) {
        format!("v{tag}")
    } else {
        tag.to_string()
    })
}

/// Order a release tag: `vYYYYMMDD`, or `vYYYYMMDD.N` for a same-day patch.
fn release_order(tag: &str) -> Option<(u32, u32)> {
    let rest = tag.strip_prefix('v')?;
    let (date, patch) = match rest.split_once('.') {
        Some((date, patch)) => (date, patch.parse().ok()?),
        None => (rest, 0),
    };
    if date.len() != 8 || !date.bytes().all(|b| b.is_ascii_digit()) {
        return None;
    }
    Some((date.parse().ok()?, patch))
}

/// Whether `candidate` is a newer release than `current`. Tags that do not
/// follow the date scheme count as newer whenever they differ, so an update
/// is never withheld because a tag could not be ordered.
fn is_newer(candidate: &str, current: &str) -> bool {
    match (release_order(candidate), release_order(current)) {
        (Some(candidate), Some(current)) => candidate > current,
        _ => candidate != current,
    }
}

/// The binary that is running, with symlinks resolved, so the file replaced
/// is the real one rather than a link to it.
fn current_exe() -> Result<PathBuf> {
    let exe = std::env::current_exe().context("find the running memorylake binary")?;
    // Windows paths stay as they are: canonicalizing yields a `\\?\` path,
    // and the replacement there works on the running executable anyway.
    #[cfg(unix)]
    let exe = exe
        .canonicalize()
        .with_context(|| format!("resolve {}", exe.display()))?;
    Ok(exe)
}

fn http_client(redirects: Policy) -> Result<Client> {
    Client::builder()
        .user_agent(format!("memorylake-cli/{}", crate::VERSION))
        .redirect(redirects)
        .connect_timeout(Duration::from_secs(30))
        .timeout(Duration::from_secs(600))
        .build()
        .context("build HTTP client")
}

/// Resolve the newest release from the redirect `<releases>/latest` answers
/// with, rather than the GitHub API: no rate limit, and the same lookup the
/// installers make.
fn latest_tag(releases: &str) -> Result<String> {
    let url = format!("{releases}/latest");
    let response = http_client(Policy::none())?
        .get(&url)
        .send()
        .with_context(|| format!("look up the latest release at {url}"))?;
    let status = response.status();
    let location = response
        .headers()
        .get(LOCATION)
        .and_then(|value| value.to_str().ok())
        .filter(|_| status.is_redirection());
    let Some(location) = location else {
        bail!("could not find the latest release: {url} answered HTTP {status} without a redirect");
    };
    tag_from_location(location).ok_or_else(|| {
        anyhow::anyhow!(
            "no release is published yet ({url} redirected to {location}); pass --version <tag>"
        )
    })
}

/// The tag in a `…/releases/tag/<tag>` URL.
fn tag_from_location(location: &str) -> Option<String> {
    let (_, tag) = location.trim_end_matches('/').rsplit_once("/tag/")?;
    parse_tag(tag).ok()
}

/// The archive name the release workflow gives the build for `target`.
fn asset_name(tag: &str, target: &str) -> String {
    let ext = if target.contains("-windows-") {
        "zip"
    } else {
        "tar.gz"
    };
    format!("memorylake-{tag}-{target}.{ext}")
}

/// Download this platform's archive of `tag`, verify it, and return the
/// binary inside.
fn download_release(releases: &str, tag: &str) -> Result<Vec<u8>> {
    let asset = asset_name(tag, TARGET);
    let base = format!("{releases}/download/{tag}");
    let http = http_client(Policy::default())?;

    eprintln!("downloading {asset}");
    let archive = download(&http, &format!("{base}/{asset}")).with_context(|| {
        format!("check that {tag} exists and publishes a build for {TARGET}: {releases}")
    })?;
    let sums = download(&http, &format!("{base}/{asset}.sha256"))
        .with_context(|| format!("download the checksum for {asset}"))?;
    verify_checksum(&asset, &archive, &String::from_utf8_lossy(&sums))?;
    extract_binary(&asset, &archive)
}

fn download(http: &Client, url: &str) -> Result<Vec<u8>> {
    let response = http
        .get(url)
        .send()
        .with_context(|| format!("download {url}"))?;
    let status = response.status();
    if !status.is_success() {
        bail!("download {url}: HTTP {status}");
    }
    let mut bytes = Vec::new();
    response
        .take(MAX_DOWNLOAD_BYTES + 1)
        .read_to_end(&mut bytes)
        .with_context(|| format!("download {url}"))?;
    if bytes.len() as u64 > MAX_DOWNLOAD_BYTES {
        bail!("download {url}: larger than {MAX_DOWNLOAD_BYTES} bytes; refusing it");
    }
    Ok(bytes)
}

/// Check `archive` against a `shasum`-style line: `<hex>  <file name>`.
fn verify_checksum(asset: &str, archive: &[u8], sums: &str) -> Result<()> {
    let mut fields = sums.split_whitespace();
    let expected = fields.next().unwrap_or_default().to_ascii_lowercase();
    if expected.len() != 64 || !expected.bytes().all(|b| b.is_ascii_hexdigit()) {
        bail!("the checksum published for {asset} is not a SHA-256 digest");
    }
    // `*` marks binary mode in `shasum` output; the name is the same file.
    if let Some(name) = fields.next()
        && name.trim_start_matches('*') != asset
    {
        bail!("the checksum published for {asset} names another file ({name})");
    }
    let actual: String = ring::digest::digest(&ring::digest::SHA256, archive)
        .as_ref()
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect();
    if actual != expected {
        bail!("checksum verification failed for {asset}; refusing to install it");
    }
    Ok(())
}

/// Take the `memorylake` binary out of a release archive, wherever it sits in
/// it: the releases put it in a directory named after the archive.
fn extract_binary(asset: &str, archive: &[u8]) -> Result<Vec<u8>> {
    let (found, name) = if asset.ends_with(".zip") {
        (from_zip(archive, "memorylake.exe")?, "memorylake.exe")
    } else {
        (from_tar_gz(archive, "memorylake")?, "memorylake")
    };
    found.with_context(|| format!("{asset} does not contain {name}"))
}

fn from_tar_gz(archive: &[u8], name: &str) -> Result<Option<Vec<u8>>> {
    let mut tar = tar::Archive::new(flate2::read::GzDecoder::new(archive));
    for entry in tar.entries().context("read the release archive")? {
        let entry = entry.context("read the release archive")?;
        let is_binary = entry.header().entry_type().is_file()
            && entry
                .path()
                .ok()
                .is_some_and(|path| path.file_name().is_some_and(|file| file == name));
        if is_binary {
            return read_bounded(entry).map(Some);
        }
    }
    Ok(None)
}

fn from_zip(archive: &[u8], name: &str) -> Result<Option<Vec<u8>>> {
    let mut zip = zip::ZipArchive::new(Cursor::new(archive)).context("read the release archive")?;
    for index in 0..zip.len() {
        let file = zip.by_index(index).context("read the release archive")?;
        // Windows PowerShell 5.1's `Compress-Archive` separates with `\`.
        let path = file.name().context("read the release archive")?;
        let is_binary = path.rsplit(['/', '\\']).next() == Some(name);
        if file.is_file() && is_binary {
            return read_bounded(file).map(Some);
        }
    }
    Ok(None)
}

fn read_bounded(reader: impl Read) -> Result<Vec<u8>> {
    let mut bytes = Vec::new();
    reader
        .take(MAX_DOWNLOAD_BYTES + 1)
        .read_to_end(&mut bytes)
        .context("unpack the release archive")?;
    if bytes.len() as u64 > MAX_DOWNLOAD_BYTES {
        bail!("the binary in the release archive is implausibly large; refusing it");
    }
    Ok(bytes)
}

/// The new binary, written beside the one it replaces and removed again
/// unless it is moved into place.
///
/// Staging in the same directory makes the final rename atomic, proves the
/// directory is writable before anything is replaced, and avoids a temporary
/// directory mounted `noexec`, where the check run could not start it.
struct Staged(PathBuf);

impl Staged {
    fn write(exe: &Path, binary: &[u8]) -> Result<Self> {
        let dir = exe
            .parent()
            .context("the running binary has no parent directory")?;
        let suffix = if cfg!(windows) { ".exe" } else { "" };
        let path = dir.join(format!(".memorylake.update-{}{suffix}", std::process::id()));
        let staged = Self(path);
        if let Err(err) = fs::write(&staged.0, binary) {
            if err.kind() == std::io::ErrorKind::PermissionDenied {
                bail!(
                    "cannot write to {}, where this memorylake is installed: permission denied\n\
                     run it again as a user who can change that directory, or reinstall with \
                     the installer into a directory you own",
                    dir.display()
                );
            }
            return Err(err).with_context(|| format!("write {}", staged.0.display()));
        }
        // Keep the old binary's mode, so a shared install stays usable by
        // whoever could run it before.
        #[cfg(unix)]
        {
            let mode = fs::metadata(exe)
                .with_context(|| format!("read {}", exe.display()))?
                .permissions();
            fs::set_permissions(&staged.0, mode)
                .with_context(|| format!("set permissions on {}", staged.0.display()))?;
        }
        Ok(staged)
    }

    /// Move the staged binary over `exe`.
    ///
    /// A rename leaves this running process on the old file, where writing
    /// into it would fail (`ETXTBSY`) or, on macOS, invalidate its signature.
    #[cfg(unix)]
    fn swap_into(self, exe: &Path) -> Result<()> {
        fs::rename(&self.0, exe)
            .with_context(|| format!("move {} to {}", self.0.display(), exe.display()))
    }

    /// Windows cannot rename over a running executable; `self_replace` moves
    /// the running one aside first and cleans it up after exit.
    #[cfg(windows)]
    fn swap_into(self, exe: &Path) -> Result<()> {
        self_replace::self_replace(&self.0).with_context(|| format!("replace {}", exe.display()))
    }
}

impl Drop for Staged {
    fn drop(&mut self) {
        // Gone already once renamed into place; best effort otherwise.
        let _ = fs::remove_file(&self.0);
    }
}

/// Run the new binary before trusting it: a build that cannot start here, or
/// one that is not the release asked for, must not replace a working one.
fn check_runs(binary: &Path, tag: &str) -> Result<()> {
    let output = Command::new(binary)
        .arg("version")
        .stdin(Stdio::null())
        .output()
        .with_context(|| format!("run the downloaded {tag} to check it"))?;
    if !output.status.success() {
        bail!(
            "the downloaded {tag} does not run here ({}); keeping the installed version\n{}",
            output.status,
            String::from_utf8_lossy(&output.stderr).trim()
        );
    }
    let reported = String::from_utf8_lossy(&output.stdout);
    let reported = reported.trim();
    if reported != tag {
        bail!("the downloaded {tag} reports itself as `{reported}`; refusing to install it");
    }
    Ok(())
}

/// Bring installed coding-agent skills up to the new binary's.
///
/// Only targets that already hold skills from `agent-skills install` are
/// refreshed: an update should not start writing into agents the user never
/// installed skills for, or ones they uninstalled them from. The new binary
/// does the work, since the skills it carries are the ones to install.
///
/// Never fails the update; the binary is already replaced.
fn refresh_agent_skills(exe: &Path) -> Option<serde_json::Value> {
    let targets = match agent_skills::installed_targets() {
        Ok(targets) => targets,
        Err(err) => {
            eprintln!("warning: could not look for installed agent skills: {err:#}");
            return None;
        }
    };
    if targets.is_empty() {
        return None;
    }
    let mut command = Command::new(exe);
    command.args(["agent-skills", "install"]);
    for target in targets {
        command.args(["--target", target]);
    }
    // Standard output is the JSON report, folded into this command's own; the
    // per-directory lines on standard error are for whoever is watching.
    let output = command
        .stdin(Stdio::null())
        .stderr(Stdio::inherit())
        .output();
    match output {
        Ok(output) if output.status.success() => serde_json::from_slice(&output.stdout).ok(),
        _ => {
            eprintln!(
                "warning: could not refresh the skills installed into coding agents; \
                 run `memorylake agent-skills install` to retry"
            );
            None
        }
    }
}

#[cfg(test)]
mod tests {
    use std::io::Write;
    use std::net::TcpListener;
    use std::sync::{Arc, Mutex};

    use super::*;

    #[test]
    fn tags_gain_their_v_and_refuse_url_syntax() {
        assert_eq!(parse_tag("v20261117").as_deref(), Ok("v20261117"));
        assert_eq!(parse_tag(" 20261117.1 ").as_deref(), Ok("v20261117.1"));
        assert!(parse_tag("").is_err());
        assert!(parse_tag("v1/../x").is_err());
        assert!(parse_tag("v1?x").is_err());
    }

    #[test]
    fn releases_order_by_date_then_patch() {
        assert!(is_newer("v20261118", "v20261117"));
        assert!(is_newer("v20261117.1", "v20261117"));
        assert!(is_newer("v20261117.10", "v20261117.9"));
        assert!(!is_newer("v20261117", "v20261117"));
        assert!(!is_newer("v20261116", "v20261117.3"));
        // Unorderable tags never withhold an update, and never repeat one.
        assert!(is_newer("nightly", "v20261117"));
        assert!(!is_newer("nightly", "nightly"));
        assert!(
            is_newer("v2026", "v20261117"),
            "a short date is not ordered"
        );
    }

    #[test]
    fn the_latest_tag_comes_from_the_redirect() {
        assert_eq!(
            tag_from_location(
                "https://github.com/memorylake-ai/memorylake-cli/releases/tag/v20261117"
            )
            .as_deref(),
            Some("v20261117")
        );
        assert_eq!(
            tag_from_location("/releases/tag/v20261117.2/").as_deref(),
            Some("v20261117.2")
        );
        // GitHub sends a repository without releases to the releases page.
        assert_eq!(
            tag_from_location("https://github.com/memorylake-ai/memorylake-cli/releases"),
            None
        );
    }

    #[test]
    fn assets_are_named_like_the_release_workflow_names_them() {
        assert_eq!(
            asset_name("v20261117", "aarch64-apple-darwin"),
            "memorylake-v20261117-aarch64-apple-darwin.tar.gz"
        );
        assert_eq!(
            asset_name("v20261117", "x86_64-pc-windows-msvc"),
            "memorylake-v20261117-x86_64-pc-windows-msvc.zip"
        );
    }

    fn sha256_hex(bytes: &[u8]) -> String {
        ring::digest::digest(&ring::digest::SHA256, bytes)
            .as_ref()
            .iter()
            .map(|byte| format!("{byte:02x}"))
            .collect()
    }

    #[test]
    fn checksums_must_match_the_archive_they_name() {
        let archive = b"payload";
        let hex = sha256_hex(archive);
        let asset = "memorylake-v1-x.tar.gz";
        verify_checksum(asset, archive, &format!("{hex}  {asset}\n")).expect("matches");
        verify_checksum(asset, archive, &format!("{}  *{asset}", hex.to_uppercase()))
            .expect("case and binary-mode marker do not matter");
        verify_checksum(asset, archive, &hex).expect("a bare digest is enough");

        let err = verify_checksum(asset, b"tampered", &format!("{hex}  {asset}")).unwrap_err();
        assert!(err.to_string().contains("verification failed"), "{err}");
        let err = verify_checksum(asset, archive, &format!("{hex}  other.tar.gz")).unwrap_err();
        assert!(err.to_string().contains("another file"), "{err}");
        let err = verify_checksum(asset, archive, "<html>not found</html>").unwrap_err();
        assert!(err.to_string().contains("not a SHA-256"), "{err}");
    }

    fn tar_gz(entries: &[(&str, &[u8])]) -> Vec<u8> {
        let encoder = flate2::write::GzEncoder::new(Vec::new(), flate2::Compression::fast());
        let mut builder = tar::Builder::new(encoder);
        for (path, bytes) in entries {
            let mut header = tar::Header::new_gnu();
            header.set_size(bytes.len() as u64);
            header.set_mode(0o755);
            header.set_cksum();
            builder
                .append_data(&mut header, path, *bytes)
                .expect("append");
        }
        builder
            .into_inner()
            .expect("finish tar")
            .finish()
            .expect("finish gzip")
    }

    fn zip_of(entries: &[(&str, &[u8])]) -> Vec<u8> {
        let mut writer = zip::ZipWriter::new(Cursor::new(Vec::new()));
        let options = zip::write::SimpleFileOptions::default()
            .compression_method(zip::CompressionMethod::Deflated);
        for (path, bytes) in entries {
            writer.start_file(*path, options).expect("start file");
            writer.write_all(bytes).expect("write file");
        }
        writer.finish().expect("finish zip").into_inner()
    }

    #[test]
    fn the_binary_is_found_inside_a_tar_gz() {
        let archive = tar_gz(&[
            ("memorylake-v1-x/README.md", b"readme"),
            ("memorylake-v1-x/memorylake", b"binary"),
        ]);
        let binary = extract_binary("memorylake-v1-x.tar.gz", &archive).expect("extract");
        assert_eq!(binary, b"binary");

        let archive = tar_gz(&[("memorylake-v1-x/README.md", b"readme")]);
        let err = extract_binary("memorylake-v1-x.tar.gz", &archive).unwrap_err();
        assert!(
            err.to_string().contains("does not contain memorylake"),
            "{err}"
        );
    }

    #[test]
    fn the_binary_is_found_inside_a_zip_whichever_separator_it_uses() {
        for path in [
            "memorylake-v1-x/memorylake.exe",
            "memorylake-v1-x\\memorylake.exe",
        ] {
            let archive = zip_of(&[("memorylake-v1-x/README.md", b"readme"), (path, b"exe")]);
            let binary = extract_binary("memorylake-v1-x.zip", &archive).expect("extract");
            assert_eq!(binary, b"exe", "{path}");
        }
        let archive = zip_of(&[("memorylake-v1-x/README.md", b"readme")]);
        let err = extract_binary("memorylake-v1-x.zip", &archive).unwrap_err();
        assert!(
            err.to_string().contains("does not contain memorylake.exe"),
            "{err}"
        );
    }

    /// A loopback server answering GET by path with `(status line rest,
    /// body)`; unknown paths get 404. Returns its base URL and the paths it
    /// was asked for.
    fn serve(routes: Vec<(String, String, Vec<u8>)>) -> (String, Arc<Mutex<Vec<String>>>) {
        let listener = TcpListener::bind("127.0.0.1:0").expect("bind");
        let base = format!("http://{}", listener.local_addr().expect("address"));
        let requested = Arc::new(Mutex::new(Vec::new()));
        let log = Arc::clone(&requested);
        std::thread::spawn(move || {
            for stream in listener.incoming() {
                let Ok(mut stream) = stream else { return };
                let mut request = Vec::new();
                let mut buf = [0u8; 1024];
                while !request.windows(4).any(|w| w == b"\r\n\r\n") {
                    match stream.read(&mut buf) {
                        Ok(0) | Err(_) => break,
                        Ok(n) => request.extend_from_slice(&buf[..n]),
                    }
                }
                let request = String::from_utf8_lossy(&request);
                let path = request.split_whitespace().nth(1).unwrap_or("").to_string();
                log.lock().expect("log").push(path.clone());
                let (status, body) = routes
                    .iter()
                    .find(|(route, _, _)| *route == path)
                    .map(|(_, status, body)| (status.clone(), body.clone()))
                    .unwrap_or_else(|| ("404 Not Found".to_string(), Vec::new()));
                let head = format!(
                    "HTTP/1.1 {status}\r\ncontent-length: {}\r\nconnection: close\r\n\r\n",
                    body.len()
                );
                let _ = stream.write_all(head.as_bytes());
                let _ = stream.write_all(&body);
            }
        });
        (base, requested)
    }

    /// Releases whose `/latest` redirects to `latest`; nothing to download.
    fn releases_with_latest(latest: &str) -> (String, Arc<Mutex<Vec<String>>>) {
        let (base, requested) = serve(vec![(
            "/releases/latest".to_string(),
            format!("302 Found\r\nlocation: /releases/tag/{latest}"),
            Vec::new(),
        )]);
        (format!("{base}/releases"), requested)
    }

    fn args() -> UpdateArgs {
        UpdateArgs {
            version: None,
            check: false,
            force: false,
            no_skills: true,
        }
    }

    fn installed(release: &str, releases: String) -> Installation {
        Installation {
            running: release.to_string(),
            release: Some(release.to_string()),
            exe: PathBuf::from("/nonexistent/memorylake"),
            releases,
        }
    }

    #[test]
    fn the_latest_release_installed_already_is_left_alone() {
        let (releases, requested) = releases_with_latest("v20261117");
        let installation = installed("v20261117", releases);

        let report = update(&args(), &installation).expect("nothing to do");
        assert!(!report.update_available && !report.updated, "{report:?}");
        let check = UpdateArgs {
            check: true,
            ..args()
        };
        let report = update(&check, &installation).expect("check");
        assert!(!report.update_available && !report.updated, "{report:?}");
        // Nothing beyond the lookups: no download was attempted.
        assert_eq!(
            *requested.lock().expect("log"),
            ["/releases/latest", "/releases/latest"]
        );
    }

    #[test]
    fn a_build_newer_than_the_latest_release_is_not_moved_back() {
        let (releases, _) = releases_with_latest("v20261102");
        let report = update(&args(), &installed("v20261117", releases)).expect("nothing to do");
        assert_eq!(report.to, "v20261102");
        assert!(!report.update_available && !report.updated, "{report:?}");
    }

    #[test]
    fn an_explicit_version_already_installed_needs_no_lookup() {
        let installation = installed("v20261117", "http://127.0.0.1:9/releases".to_string());
        let explicit = UpdateArgs {
            version: Some("v20261117".to_string()),
            ..args()
        };
        let report = update(&explicit, &installation).expect("nothing to do");
        assert!(!report.update_available && !report.updated, "{report:?}");
    }

    #[test]
    fn latest_must_redirect_to_a_tag() {
        let (base, _) = serve(vec![(
            "/releases/latest".to_string(),
            "200 OK".to_string(),
            b"a page".to_vec(),
        )]);
        let err = latest_tag(&format!("{base}/releases")).unwrap_err();
        assert!(err.to_string().contains("without a redirect"), "{err}");

        // What GitHub answers for a repository with no release yet.
        let (base, _) = serve(vec![(
            "/releases/latest".to_string(),
            "302 Found\r\nlocation: /releases".to_string(),
            Vec::new(),
        )]);
        let err = latest_tag(&format!("{base}/releases")).unwrap_err();
        assert!(err.to_string().contains("no release is published"), "{err}");
    }

    /// A fresh scratch directory.
    #[cfg(unix)]
    fn scratch_dir(tag: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!(
            "memorylake-self-update-{tag}-{}-{:?}",
            std::process::id(),
            std::time::SystemTime::now()
        ));
        fs::create_dir_all(&dir).expect("create scratch dir");
        dir
    }

    #[cfg(unix)]
    #[test]
    fn a_binary_that_fails_to_run_is_refused() {
        use std::os::unix::fs::PermissionsExt;

        let dir = scratch_dir("fails");
        let binary = dir.join("memorylake");
        fs::write(&binary, "#!/bin/sh\necho broken >&2\nexit 3\n").expect("write");
        fs::set_permissions(&binary, fs::Permissions::from_mode(0o755)).expect("chmod");
        let err = check_runs(&binary, "v20261117").unwrap_err();
        assert!(err.to_string().contains("does not run here"), "{err}");
        assert!(err.to_string().contains("broken"), "{err}");
        let _ = fs::remove_dir_all(&dir);
    }

    #[cfg(unix)]
    #[test]
    fn an_unwritable_install_directory_is_explained() {
        use std::os::unix::fs::PermissionsExt;

        let dir = scratch_dir("readonly");
        let exe = dir.join("memorylake");
        fs::write(&exe, "old").expect("write");
        fs::set_permissions(&dir, fs::Permissions::from_mode(0o555)).expect("chmod");
        let result = Staged::write(&exe, b"new");
        fs::set_permissions(&dir, fs::Permissions::from_mode(0o755)).expect("chmod back");
        // Root writes anywhere; there is nothing to refuse then.
        if let Err(err) = result {
            assert!(err.to_string().contains("permission denied"), "{err}");
            assert!(
                err.to_string().contains(&dir.display().to_string()),
                "{err}"
            );
        }
        let _ = fs::remove_dir_all(&dir);
    }
}
