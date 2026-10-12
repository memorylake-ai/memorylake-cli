//! CLI subcommand handlers.

pub mod access;
pub mod access_group;
pub mod actor;
pub mod agent;
pub mod api_key;
pub mod audit_log;
pub mod auth;
pub mod billing;
pub mod boundary;
pub mod conversation;
pub mod datasource;
pub mod db_connection;
mod download;
pub mod fact;
mod input;
pub mod invitation;
pub mod library;
pub mod member;
pub mod opendata;
pub mod permission;
mod scope_input;

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

/// Print a payload like [`print_json`], and its `ETag` on standard error.
///
/// The `ETag` is what the matching write takes back as `--if-match` to refuse
/// overwriting a change made since. Standard error keeps standard output a
/// single JSON document for whoever parses it.
pub fn print_json_with_etag<T: serde::Serialize>(data: &T, etag: Option<&str>) -> Result<()> {
    print_json(data)?;
    if let Some(etag) = etag {
        eprintln!("ETag: {etag}");
    }
    Ok(())
}

/// Accept an audit-log reason: non-blank, and at most 255 bytes of UTF-8 —
/// the documented cap of the scope endpoints, applied to every reason so the
/// limit is the same wherever one is asked for.
pub fn parse_reason(raw: &str) -> std::result::Result<String, String> {
    if raw.trim().is_empty() {
        return Err("must not be empty".to_string());
    }
    if raw.len() > 255 {
        return Err(format!("must be at most 255 bytes, got {}", raw.len()));
    }
    Ok(raw.to_string())
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

/// Accept an ETag for `--if-match` the way `get` printed it on standard
/// error, forgiving what a shell or a copy-paste does to it.
///
/// The server compares strong tags byte for byte, quotes included, so a
/// shell that ate the quotes (`--if-match "abc"` sends `abc`) would get a
/// 412 that wrongly says the resource changed. A bare value is quoted again,
/// and a whole `ETag: "abc"` line is accepted too.
pub fn parse_etag(raw: &str) -> std::result::Result<String, String> {
    let mut value = raw.trim();
    if let Some((name, rest)) = value.split_once(':')
        && name.trim().eq_ignore_ascii_case("etag")
    {
        value = rest.trim();
    }
    if value.is_empty() {
        return Err("must not be empty".to_string());
    }
    if value == "*" {
        return Ok(value.to_string());
    }
    // One tag or a comma-separated list; quote each bare one again.
    let tags: Vec<String> = value
        .split(',')
        .map(str::trim)
        .filter(|tag| !tag.is_empty())
        .map(|tag| {
            let (weak, opaque) = match tag.strip_prefix("W/") {
                Some(rest) => ("W/", rest),
                None => ("", tag),
            };
            if opaque.starts_with('"') {
                tag.to_string()
            } else {
                format!("{weak}\"{opaque}\"")
            }
        })
        .collect();
    if tags.is_empty() {
        return Err("must not be empty".to_string());
    }
    Ok(tags.join(", "))
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
    fn etags_survive_lost_quotes_and_whole_lines() {
        assert_eq!(parse_etag(r#""v7""#).as_deref(), Ok(r#""v7""#));
        assert_eq!(parse_etag("v7").as_deref(), Ok(r#""v7""#));
        assert_eq!(parse_etag(r#"ETag: "v7""#).as_deref(), Ok(r#""v7""#));
        assert_eq!(parse_etag("etag: v7").as_deref(), Ok(r#""v7""#));
        assert_eq!(parse_etag(r#"W/"v7""#).as_deref(), Ok(r#"W/"v7""#));
        assert_eq!(parse_etag("*").as_deref(), Ok("*"));
        assert_eq!(parse_etag("W/v7").as_deref(), Ok(r#"W/"v7""#));
        assert_eq!(parse_etag("a, b").as_deref(), Ok(r#""a", "b""#));
        assert_eq!(parse_etag(r#""a", "b""#).as_deref(), Ok(r#""a", "b""#));
        assert!(parse_etag(",").is_err());
        assert!(parse_etag(" ").is_err());
    }

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
