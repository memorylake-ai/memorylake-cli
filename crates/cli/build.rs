//! Embed the repository's `skills/` directory into the binary.
//!
//! `memorylake agent-skills install` writes these files into the directories
//! coding agents read skills from. Embedding them, rather than downloading
//! them at install time, keeps each skill describing exactly the commands of
//! the binary that carries it, and works offline.

use std::fs;
use std::path::{Path, PathBuf};

/// Files an operating system leaves behind that are not part of any skill.
const IGNORED_NAMES: &[&str] = &[".DS_Store", "Thumbs.db"];

fn main() {
    let manifest_dir =
        PathBuf::from(std::env::var_os("CARGO_MANIFEST_DIR").expect("cargo sets it"));
    let skills_dir = manifest_dir.join("..").join("..").join("skills");
    println!("cargo:rerun-if-changed={}", skills_dir.display());
    assert!(
        skills_dir.is_dir(),
        "skills directory {} is missing",
        skills_dir.display()
    );

    let mut files = Vec::new();
    collect_files(&skills_dir, &skills_dir, &mut files);
    files.sort();
    assert!(
        !files.is_empty(),
        "no skills found under {}",
        skills_dir.display()
    );

    let mut bundle = String::from(
        "/// Every bundled skill file: its path relative to `skills/`, with `/`\n\
         /// separators, and its contents. Sorted by path.\n\
         pub(crate) const BUNDLED_FILES: &[(&str, &[u8])] = &[\n",
    );
    for (relative, absolute) in &files {
        // `{:?}` escapes the path, which matters for Windows backslashes.
        bundle.push_str(&format!(
            "    ({relative:?}, include_bytes!({:?})),\n",
            absolute.display().to_string()
        ));
    }
    bundle.push_str("];\n");

    let out = PathBuf::from(std::env::var_os("OUT_DIR").expect("cargo sets it"));
    fs::write(out.join("agent_skills_bundle.rs"), bundle).expect("write skills bundle");
}

/// Collect every file under `dir` as (path relative to `root`, absolute path).
fn collect_files(root: &Path, dir: &Path, files: &mut Vec<(String, PathBuf)>) {
    let entries = fs::read_dir(dir).unwrap_or_else(|err| panic!("read {}: {err}", dir.display()));
    for entry in entries {
        let entry = entry.unwrap_or_else(|err| panic!("read {}: {err}", dir.display()));
        let path = entry.path();
        let name = entry.file_name();
        if IGNORED_NAMES.iter().any(|ignored| name == *ignored) {
            continue;
        }
        if path.is_dir() {
            collect_files(root, &path, files);
        } else {
            let relative = path
                .strip_prefix(root)
                .expect("walked from root")
                .components()
                .map(|part| part.as_os_str().to_str().expect("skill paths are UTF-8"))
                .collect::<Vec<_>>()
                .join("/");
            files.push((relative, path));
        }
    }
}
