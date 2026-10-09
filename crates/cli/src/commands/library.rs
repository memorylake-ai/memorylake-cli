//! `memorylake library` / `lib` commands.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use anyhow::{Context, Result, bail};
use clap::{Subcommand, ValueEnum};
use memorylake_core::api::library::{
    CreateFolderRequest, ListChildrenParams, NameConflictStrategy, ROOT_ALIAS, UploadFileRequest,
    create_folder, delete_item, delete_xattrs, get_item, list_children, set_xattrs, upload_file,
};
use memorylake_core::{Client, Paths, ResolveOverrides, resolve};

use super::parse_string_map;
use super::search::{IdList, parse_id_list};

/// Library subcommands.
#[derive(Debug, Subcommand)]
pub enum LibraryCommand {
    /// Show a file or folder.
    Get {
        /// Item id, or `MY_SPACE` for the workspace root.
        item_id: String,
    },
    /// List the contents of a folder.
    List {
        /// Folder item id, or `MY_SPACE` for the workspace root.
        #[arg(default_value = ROOT_ALIAS)]
        item_id: String,
        /// Number of items per page (the API accepts 1–50).
        #[arg(long)]
        page_size: Option<u32>,
        /// Continuation token from a previous response.
        #[arg(long)]
        continuation_token: Option<String>,
        /// Extended-attribute keys to include on each item (comma-separated).
        /// `x_attrs` then holds those keys only, instead of the server's
        /// default set.
        #[arg(long, value_name = "KEYS", value_parser = parse_id_list)]
        xattr_keys: Option<IdList>,
    },
    /// Create a folder.
    Mkdir {
        /// Folder name.
        name: String,
        /// Parent folder item id.
        #[arg(long, default_value = ROOT_ALIAS)]
        parent: String,
        /// What to do if the name is taken. Folders accept only
        /// `rename` and `deny`.
        #[arg(long = "on-conflict", value_enum)]
        on_conflict: Option<ConflictStrategyArg>,
        /// Extended attributes to create the folder with, as a JSON object of
        /// strings, e.g. '{"team":"core"}'.
        #[arg(long, value_name = "JSON", value_parser = parse_string_map)]
        xattrs: Option<BTreeMap<String, String>>,
    },
    /// Upload a local file.
    Upload {
        /// Local file to upload.
        file: PathBuf,
        /// Destination folder item id.
        #[arg(long, default_value = ROOT_ALIAS)]
        parent: String,
        /// Name to store it under (defaults to the local file name).
        #[arg(long)]
        name: Option<String>,
        /// What to do if the name is taken.
        #[arg(long = "on-conflict", value_enum)]
        on_conflict: Option<ConflictStrategyArg>,
        /// Extended attributes to create the file with, as a JSON object of
        /// strings, e.g. '{"team":"core"}'.
        #[arg(long, value_name = "JSON", value_parser = parse_string_map)]
        xattrs: Option<BTreeMap<String, String>>,
    },
    /// Set or remove extended attributes on a file or folder.
    Xattr {
        #[command(subcommand)]
        command: XattrCommand,
    },
    /// Delete a file or folder.
    ///
    /// Deleting a folder recursively removes everything inside it. This is
    /// irreversible and is not confirmed.
    Delete {
        /// Item id to delete.
        item_id: String,
    },
}

/// `library xattr` subcommands.
///
/// Server-managed keys (`x_source`, `x_thumbnail_uri`, …) cannot be changed:
/// the server accepts the request and leaves them as they were. System
/// folders such as `MY_SPACE` refuse changes outright.
#[derive(Debug, Subcommand)]
pub enum XattrCommand {
    /// Add or overwrite extended attributes.
    ///
    /// Merges: only the keys given are written, every other attribute is
    /// left as it was. Read them back with `library get`.
    Set {
        /// Item id of the file or folder.
        item_id: String,
        /// Attributes as a non-empty JSON object of strings,
        /// e.g. '{"team":"core"}'.
        #[arg(long, value_name = "JSON", value_parser = parse_xattrs_to_set)]
        attrs: BTreeMap<String, String>,
    },
    /// Remove extended attributes by key.
    ///
    /// Irreversible and not confirmed. A key the item does not carry is
    /// ignored.
    Delete {
        /// Item id of the file or folder.
        item_id: String,
        /// Keys to remove (comma-separated). A key containing a comma cannot
        /// be named.
        #[arg(long, value_name = "KEYS", value_parser = parse_id_list)]
        keys: IdList,
    },
}

/// `--attrs` for `xattr set`: a string map with at least one entry, since the
/// server rejects an empty one.
fn parse_xattrs_to_set(raw: &str) -> std::result::Result<BTreeMap<String, String>, String> {
    let attrs = parse_string_map(raw)?;
    if attrs.is_empty() {
        return Err("must name at least one attribute".to_string());
    }
    Ok(attrs)
}

/// CLI spelling of the server's name-conflict strategies.
#[derive(Debug, Clone, Copy, PartialEq, Eq, ValueEnum)]
pub enum ConflictStrategyArg {
    /// Append a `_N` suffix. Server default.
    Rename,
    /// Fail with a conflict error.
    Deny,
    /// Files only: replace content, keeping the same item id.
    Overwrite,
    /// Files only: delete and recreate, yielding a new item id.
    Replace,
}

impl From<ConflictStrategyArg> for NameConflictStrategy {
    fn from(value: ConflictStrategyArg) -> Self {
        match value {
            ConflictStrategyArg::Rename => Self::Rename,
            ConflictStrategyArg::Deny => Self::Deny,
            ConflictStrategyArg::Overwrite => Self::Overwrite,
            ConflictStrategyArg::Replace => Self::Replace,
        }
    }
}

/// Execute a `library` subcommand.
pub fn run(
    command: LibraryCommand,
    profile: Option<String>,
    base_url: Option<String>,
) -> Result<()> {
    let paths = Paths::default_home().context("resolve MemoryLake config paths")?;
    let runtime = resolve(&paths, &ResolveOverrides { profile, base_url })
        .context("resolve API credentials")?;
    let client = Client::new(&runtime.base_url, &runtime.api_key).context("build API client")?;

    match command {
        LibraryCommand::Get { item_id } => {
            let data = get_item(&client, &item_id).context("get library item")?;
            println!("{}", serde_json::to_string_pretty(&data)?);
        }
        LibraryCommand::List {
            item_id,
            page_size,
            continuation_token,
            xattr_keys,
        } => {
            let data = list_children(
                &client,
                &item_id,
                &ListChildrenParams {
                    page_size,
                    continuation_token,
                    with_xattr_keys: xattr_keys.map(|keys| keys.0),
                },
            )
            .context("list library items")?;
            println!("{}", serde_json::to_string_pretty(&data)?);
        }
        LibraryCommand::Mkdir {
            name,
            parent,
            on_conflict,
            xattrs,
        } => {
            let data = create_folder(
                &client,
                &CreateFolderRequest {
                    parent_item_id: parent,
                    name,
                    name_conflict_strategy: on_conflict.map(Into::into),
                    x_attrs: xattrs,
                },
            )
            .context("create library folder")?;
            println!("{}", serde_json::to_string_pretty(&data)?);
        }
        LibraryCommand::Upload {
            file,
            parent,
            name,
            on_conflict,
            xattrs,
        } => {
            let name = match name {
                Some(name) => name,
                None => default_upload_name(&file)?,
            };
            let data = upload_file(
                &client,
                &UploadFileRequest {
                    source: file,
                    parent_item_id: parent,
                    name,
                    name_conflict_strategy: on_conflict.map(Into::into),
                    x_attrs: xattrs,
                },
            )
            .context("upload file to library")?;
            println!("{}", serde_json::to_string_pretty(&data)?);
        }
        LibraryCommand::Xattr {
            command: XattrCommand::Set { item_id, attrs },
        } => {
            set_xattrs(&client, &item_id, &attrs)
                .with_context(|| format!("set extended attributes on `{item_id}`"))?;
            let keys: Vec<&str> = attrs.keys().map(String::as_str).collect();
            println!(
                "Set {} extended attribute(s) on `{item_id}`: {}",
                keys.len(),
                keys.join(", ")
            );
        }
        LibraryCommand::Xattr {
            command: XattrCommand::Delete { item_id, keys },
        } => {
            delete_xattrs(&client, &item_id, &keys.0)
                .with_context(|| format!("delete extended attributes from `{item_id}`"))?;
            println!(
                "Deleted extended attribute(s) {} from `{item_id}`",
                keys.0.join(", ")
            );
        }
        LibraryCommand::Delete { item_id } => {
            delete_item(&client, &item_id).context("delete library item")?;
            println!("Deleted item `{item_id}`");
        }
    }

    Ok(())
}

/// Derive the Library name from a local path.
fn default_upload_name(file: &Path) -> Result<String> {
    match file.file_name().and_then(|name| name.to_str()) {
        Some(name) if !name.is_empty() => Ok(name.to_string()),
        // `..`, a bare `/`, or a non-UTF-8 name: the user has to say what to
        // call it rather than us inventing something.
        _ => bail!(
            "cannot derive a name from `{}`; pass --name",
            file.display()
        ),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn upload_name_defaults_to_the_local_file_name() {
        assert_eq!(
            default_upload_name(Path::new("/tmp/reports/q4.pdf")).unwrap(),
            "q4.pdf"
        );
        assert_eq!(default_upload_name(Path::new("q4.pdf")).unwrap(), "q4.pdf");
    }

    #[test]
    fn upload_name_requires_an_override_when_undecidable() {
        let err = default_upload_name(Path::new("..")).expect_err("`..` has no file name");
        assert!(err.to_string().contains("--name"));
    }

    #[test]
    fn conflict_strategy_arg_maps_onto_the_core_enum() {
        assert_eq!(
            NameConflictStrategy::from(ConflictStrategyArg::Overwrite),
            NameConflictStrategy::Overwrite
        );
        assert_eq!(
            NameConflictStrategy::from(ConflictStrategyArg::Deny),
            NameConflictStrategy::Deny
        );
    }
}
