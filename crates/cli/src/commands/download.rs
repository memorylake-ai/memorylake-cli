//! Saving a downloaded file: to stdout, or into place under a name chosen
//! from `--output` and what the server reports.

use std::fs;
use std::io::Write;
use std::path::{Path, PathBuf};

use anyhow::{Context, Result, bail};
use memorylake_core::Downloaded;

/// Where a download goes and what it may overwrite.
pub struct SaveTo<'a> {
    /// `--output`: a file path, a directory, or `-` for stdout.
    pub output: Option<&'a str>,
    /// `--force`: overwrite an existing destination.
    pub force: bool,
    /// The name to use when the server reports none.
    pub default_name: Option<String>,
}

/// Write a download to stdout, or to a file chosen from `--output` and the
/// name the server reports.
///
/// `what` names the download in errors ("download {what}"); `tag` names the
/// temporary file. Downloads to a temporary file in the destination
/// directory and renames it into place, so an interrupted transfer leaves no
/// half-written file where a complete one is expected. Streaming to stdout
/// skips all of that: the caller is piping, and there is nothing to name or
/// rename.
pub fn save_download(
    to: SaveTo<'_>,
    what: &str,
    tag: &str,
    fetch: impl FnOnce(&mut dyn Write) -> memorylake_core::Result<Downloaded>,
) -> Result<()> {
    if to.output == Some("-") {
        let stdout = std::io::stdout();
        let mut handle = stdout.lock();
        let downloaded = fetch(&mut handle).with_context(|| format!("download {what}"))?;
        handle.flush().context("flush stdout")?;
        // The bytes are the output, so the summary goes to stderr where it
        // cannot corrupt a pipe.
        eprintln!("wrote {} bytes to stdout", downloaded.bytes);
        return Ok(());
    }

    // Work out the directory up front: the temporary file has to live on the
    // same filesystem as the destination for the rename to be atomic.
    let requested = to.output.map(PathBuf::from);
    let explicit_file = match &requested {
        Some(path) if path.is_dir() => None,
        Some(path) => Some(path.clone()),
        None => None,
    };
    let directory = match (&requested, &explicit_file) {
        (Some(path), None) => path.clone(),
        (_, Some(file)) => file.parent().map(Path::to_path_buf).unwrap_or_default(),
        (None, None) => PathBuf::from("."),
    };
    let directory = if directory.as_os_str().is_empty() {
        PathBuf::from(".")
    } else {
        directory
    };

    // Fail before transferring anything when the destination is already known
    // and taken.
    if let Some(file) = &explicit_file {
        refuse_existing(file, to.force)?;
    }

    fs::create_dir_all(&directory)
        .with_context(|| format!("create directory {}", directory.display()))?;

    // The tag is caller input; keep it from naming another directory.
    let tag: String = tag
        .chars()
        .map(|c| {
            if c.is_ascii_alphanumeric() || c == '-' {
                c
            } else {
                '_'
            }
        })
        .collect();
    let temporary = directory.join(format!(".{tag}.download.{}", std::process::id()));
    let downloaded = {
        let mut file = fs::File::create(&temporary)
            .with_context(|| format!("create {}", temporary.display()))?;
        let result = fetch(&mut file).with_context(|| format!("download {what}"));
        // Remove the partial file on any failure, including a mid-transfer one.
        match result {
            Ok(downloaded) => downloaded,
            Err(err) => {
                let _ = fs::remove_file(&temporary);
                return Err(err);
            }
        }
    };

    let destination = match explicit_file {
        Some(file) => file,
        None => match downloaded.filename.or(to.default_name) {
            Some(name) => directory.join(name),
            None => {
                let _ = fs::remove_file(&temporary);
                bail!(
                    "the server did not say what this file is called; \
                     pass --output <path> to name it"
                );
            }
        },
    };

    if let Err(err) = refuse_existing(&destination, to.force) {
        let _ = fs::remove_file(&temporary);
        return Err(err);
    }

    fs::rename(&temporary, &destination).with_context(|| {
        format!(
            "move the downloaded file into place at {}",
            destination.display()
        )
    })?;

    println!(
        "downloaded {} ({} bytes)",
        destination.display(),
        downloaded.bytes
    );
    Ok(())
}

/// Refuse to clobber an existing destination unless `--force` was given.
fn refuse_existing(path: &Path, force: bool) -> Result<()> {
    if force || !path.exists() {
        return Ok(());
    }
    bail!(
        "{} already exists; pass --force to overwrite it",
        path.display()
    )
}
