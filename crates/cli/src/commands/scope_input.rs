//! The `scope set` input shared by members, roles, and access groups.

use std::path::{Path, PathBuf};

use anyhow::{Context, Result};
use clap::Args;
use memorylake_core::api::admin::{ScopeEntryInput, SetScopesRequest, parse_scope_entries};

use super::input::read_file_or_stdin;
use super::{parse_etag, parse_reason};

/// The `--help` epilogue of every `scope set`: the entry format and the
/// read-edit-write loop, which flattened arguments cannot show.
pub const SCOPE_SET_HELP: &str = "\
Entries: a JSON array of
  {\"resource_type\": \"memorylake:<type>\", \"mode\": \"all\" | \"instances\",
   \"instances\": [{\"id\": \"...\", \"workspace_id\": \"...\", \"actions\": [\"...\"]}]}
or the whole JSON `scope get` printed, edited: read-only fields are dropped.
Only the listed resource types change. A project instance's workspace_id is
filled in by the server when left out. `actions` is optional (default: every action the role allows on it). An entry
in mode `instances` must list at least one instance: leave a type out to keep
it as it is.

Read, edit, write back, refusing to overwrite a concurrent change:
  memorylake member scope get <id> > scope.json 2> etag.txt
  # edit scope.json
  memorylake member scope set <id> --scopes-file scope.json --reason \"...\" \\
      --if-match \"$(cat etag.txt)\"
(The same for `role scope` and `access-group scope`.) Resource types: the
`resource_type` values `scope get` prints, or `permission catalog`.";

/// Where the replacement scope entries come from, and why they change.
///
/// The entries are a JSON array of `{resource_type, mode, instances}` — or
/// the whole output of the matching `scope get`, edited, since read-only
/// fields are accepted and dropped; entries listing no instances must be
/// left out. Only the listed resource types change.
#[derive(Debug, Args)]
pub struct ScopeSetArgs {
    #[command(flatten)]
    source: ScopeSource,
    /// Why the change is made; recorded in the audit log.
    #[arg(long, value_parser = parse_reason)]
    reason: String,
    /// Only write if nothing changed since the read that printed this ETag
    /// (`scope get` prints `ETag: "..."` on standard error; the value, with
    /// or without its quotes, or the whole line); otherwise fail with 412.
    #[arg(long, value_name = "ETAG", value_parser = parse_etag)]
    if_match: Option<String>,
}

#[derive(Debug, Args)]
#[group(id = "scope_source", required = true, multiple = false)]
struct ScopeSource {
    /// The entries as JSON, e.g.
    /// '[{"resource_type":"memorylake:workspace","mode":"instances","instances":[{"id":"ws-1"}]}]'.
    #[arg(long, value_name = "JSON")]
    scopes: Option<String>,
    /// Read the entries from this JSON file, or `-` for standard input.
    #[arg(long, value_name = "PATH")]
    scopes_file: Option<PathBuf>,
}

impl ScopeSetArgs {
    /// Read and check the entries, before any credentials are needed.
    pub fn request(&self) -> Result<SetScopesRequest> {
        let json = match (&self.source.scopes, &self.source.scopes_file) {
            (Some(json), _) => json.clone(),
            (None, Some(path)) => read_file_or_stdin(path, "scopes file")?,
            (None, None) => unreachable!("clap requires one scope source"),
        };
        let scopes: Vec<ScopeEntryInput> =
            parse_scope_entries(&json).with_context(|| match &self.source.scopes_file {
                Some(path) if path != Path::new("-") => {
                    format!("invalid scopes in {}", path.display())
                }
                Some(_) => "invalid scopes on standard input".to_string(),
                None => "invalid --scopes".to_string(),
            })?;
        Ok(SetScopesRequest {
            reason: self.reason.clone(),
            scopes,
        })
    }

    /// The `If-Match` value, when one was given.
    pub fn if_match(&self) -> Option<&str> {
        self.if_match.as_deref()
    }
}
