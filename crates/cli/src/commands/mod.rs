//! CLI subcommand handlers.

pub mod actor;
pub mod agent;
pub mod analysis_model;
pub mod api_key;
pub mod auth;
pub mod boundary;
pub mod conversation;
pub mod fact;
pub mod industry;
pub mod invitation;
pub mod library;
pub mod member;

pub mod project;
pub mod role;
pub mod search;
pub mod skill;
pub mod team;
pub mod usage;
pub mod workspace;

use anyhow::{Context, Result, bail};
use memorylake_core::{Client, Paths, ResolveOverrides, resolve, resolve_profile_workspace};

/// Resolve credentials and build the authenticated API client.
///
/// The team-management commands share this instead of each repeating the
/// paths → resolve → client chain the older command modules carry inline.
pub fn api_client(profile: Option<String>, base_url: Option<String>) -> Result<Client> {
    let paths = Paths::default_home().context("resolve MemoryLake config paths")?;
    let runtime = resolve(&paths, &ResolveOverrides { profile, base_url })
        .context("resolve API credentials")?;
    Client::new(&runtime.base_url, &runtime.api_key).context("build API client")
}

/// Print an API payload the way every command here does: pretty JSON.
pub fn print_json<T: serde::Serialize>(data: &T) -> Result<()> {
    println!("{}", serde_json::to_string_pretty(data)?);
    Ok(())
}

/// Parse a flag value holding a JSON object of string values, such as
/// metadata or extended attributes (`'{"team":"core"}'`).
///
/// Used as a clap `value_parser`, so malformed JSON, a non-object, or a
/// non-string value is refused before credentials are even resolved. Values
/// must be strings: the API stores `string → string` maps, and silently
/// stringifying `3` or `true` would store something the caller did not write.
pub fn parse_string_map(
    raw: &str,
) -> std::result::Result<std::collections::BTreeMap<String, String>, String> {
    let value: serde_json::Value = serde_json::from_str(raw)
        .map_err(|err| format!("must be a JSON object: invalid JSON: {err}"))?;
    let serde_json::Value::Object(map) = value else {
        return Err("must be a JSON object, e.g. '{\"team\":\"core\"}'".to_string());
    };
    map.into_iter()
        .map(|(key, value)| match value {
            serde_json::Value::String(text) => Ok((key, text)),
            other => Err(format!(
                "value of `{key}` must be a JSON string, got `{other}`; quote it"
            )),
        })
        .collect()
}

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

/// Accept a non-blank flag or argument value, trimmed.
///
/// For ids and other values where an empty string is never meaningful: sent
/// as-is it would address the collection instead of an item, or be read by
/// the server as "clear".
pub fn parse_non_blank(raw: &str) -> std::result::Result<String, String> {
    let value = raw.trim();
    if value.is_empty() {
        return Err("must not be empty".to_string());
    }
    Ok(value.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn string_maps_accept_objects_of_strings() {
        let map = parse_string_map(r#"{"team":"core","tier":""}"#).expect("valid");
        assert_eq!(map["team"], "core");
        assert_eq!(map["tier"], "");
        assert!(parse_string_map("{}").expect("empty").is_empty());
    }

    #[test]
    fn string_maps_refuse_what_the_api_cannot_store() {
        let err = parse_string_map("not json").unwrap_err();
        assert!(err.contains("invalid JSON"), "{err}");
        let err = parse_string_map("[1]").unwrap_err();
        assert!(err.contains("JSON object"), "{err}");
        let err = parse_string_map(r#"{"seats":3}"#).unwrap_err();
        assert!(err.contains("`seats`") && err.contains("string"), "{err}");
    }
}
