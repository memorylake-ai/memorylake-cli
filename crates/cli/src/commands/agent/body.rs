//! Request-body construction for `agent` commands.
//!
//! Bodies are open JSON objects: documented fields are known, but any other
//! top-level key the user supplies through `--config` is forwarded to the API
//! verbatim so the CLI keeps working when the API gains fields first.

use std::path::Path;
use std::str::FromStr;

use anyhow::{Context, Result, bail};
use memorylake_core::Client;
use memorylake_core::api::agents::{
    AgentRequestBody, CONFIG_FIELDS, IDENTITY_FIELDS, get_agent, get_agent_version,
};
use serde_json::Value;

/// Version-response fields that are not valid request-body input.
const VERSION_RESPONSE_ONLY_FIELDS: &[&str] = &["version", "agent_id", "created_at"];

/// Base version for `agent version create --from-version`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum FromVersion {
    /// The agent's current highest version.
    Latest,
    /// An explicit version number.
    Number(u64),
}

impl FromStr for FromVersion {
    type Err = String;

    fn from_str(value: &str) -> std::result::Result<Self, Self::Err> {
        let trimmed = value.trim();
        if trimmed.eq_ignore_ascii_case("latest") {
            return Ok(Self::Latest);
        }
        trimmed
            .parse::<u64>()
            .map(Self::Number)
            .map_err(|_| format!("expected `latest` or a version number, found `{value}`"))
    }
}

/// Read `--config` into a JSON object, or start from an empty body.
pub fn load_config_body(path: Option<&Path>) -> Result<AgentRequestBody> {
    let Some(path) = path else {
        return Ok(AgentRequestBody::new());
    };

    let text = std::fs::read_to_string(path)
        .with_context(|| format!("read config file {}", path.display()))?;
    let value: Value = serde_json::from_str(&text)
        .with_context(|| format!("parse JSON from config file {}", path.display()))?;

    match value {
        Value::Object(map) => Ok(map),
        other => bail!(
            "config file {} must hold a JSON object at the top level, found {}",
            path.display(),
            json_kind(&other)
        ),
    }
}

/// Apply a scalar flag on top of the value loaded from `--config`.
///
/// Flags win over the file so `--config base.json --model X` behaves the way
/// the flag ordering reads.
pub fn set_scalar(body: &mut AgentRequestBody, key: &str, value: Option<String>) {
    if let Some(value) = value {
        body.insert(key.to_string(), Value::String(value));
    }
}

/// Fail when a required field is absent or blank in the assembled body.
pub fn require_field(body: &AgentRequestBody, key: &str, flag: &str) -> Result<()> {
    let present = body.get(key).is_some_and(|value| match value {
        Value::Null => false,
        Value::String(text) => !text.trim().is_empty(),
        _ => true,
    });
    if !present {
        bail!("`{key}` is required; pass --{flag} or set \"{key}\" in --config");
    }
    Ok(())
}

/// Reject configuration-class keys in an `agent update` body.
///
/// `PATCH /api/v3/agents/{id}` takes identity fields only. Catching this before
/// the request turns an opaque server rejection into an actionable message.
/// Best-effort against the documented key list, not exhaustive validation:
/// keys outside both lists still pass through.
pub fn reject_config_fields(body: &AgentRequestBody) -> Result<()> {
    let offenders: Vec<&str> = CONFIG_FIELDS
        .iter()
        .copied()
        .filter(|key| body.contains_key(*key))
        .collect();
    if offenders.is_empty() {
        return Ok(());
    }

    let (subject, verb) = if offenders.len() == 1 {
        (format!("`{}` is", offenders[0]), "field")
    } else {
        (format!("`{}` are", offenders.join("`, `")), "fields")
    };
    bail!(
        "{subject} configuration, and `agent update` changes identity only ({}).\n\
         Configuration {verb} can only change by creating a new version — use `memorylake agent version create`.",
        IDENTITY_FIELDS.join(", ")
    );
}

/// Build the base body for `agent version create --from-version`.
///
/// Fetches the referenced version and strips response-only fields so the result
/// is a valid request body. Callers then overlay `--config` and scalar flags as
/// top-level key replacement.
pub fn version_base_body(
    client: &Client,
    id: &str,
    from: &FromVersion,
) -> Result<AgentRequestBody> {
    let version = match from {
        FromVersion::Latest => {
            let agent = get_agent(client, id)
                .with_context(|| format!("look up agent `{id}` for --from-version latest"))?;
            agent.latest_version.or(agent.version).with_context(|| {
                format!("agent `{id}` has no versions yet; drop --from-version or create one first")
            })?
        }
        FromVersion::Number(number) => *number,
    };

    let base = get_agent_version(client, id, version)
        .with_context(|| format!("fetch version {version} of agent `{id}` for --from-version"))?;
    let Value::Object(mut map) =
        serde_json::to_value(&base).context("serialize base version as a request body")?
    else {
        bail!("unexpected version payload for agent `{id}`: expected a JSON object");
    };
    for field in VERSION_RESPONSE_ONLY_FIELDS {
        map.remove(*field);
    }
    Ok(map)
}

fn json_kind(value: &Value) -> &'static str {
    match value {
        Value::Null => "null",
        Value::Bool(_) => "a boolean",
        Value::Number(_) => "a number",
        Value::String(_) => "a string",
        Value::Array(_) => "an array",
        Value::Object(_) => "an object",
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;
    use std::path::PathBuf;
    use std::sync::atomic::{AtomicU64, Ordering};

    fn temp_json(contents: &str) -> PathBuf {
        static NEXT: AtomicU64 = AtomicU64::new(0);
        let path = std::env::temp_dir().join(format!(
            "memorylake-agent-body-{}-{}.json",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        ));
        let mut file = std::fs::File::create(&path).expect("create temp config");
        file.write_all(contents.as_bytes()).expect("write config");
        path
    }

    #[test]
    fn load_config_body_without_a_path_is_empty() {
        assert!(load_config_body(None).unwrap().is_empty());
    }

    #[test]
    fn load_config_body_reads_a_json_object() {
        let path = temp_json(r#"{"name":"Support","policies":{"max_turns":4}}"#);
        let body = load_config_body(Some(&path)).unwrap();
        assert_eq!(body["name"], "Support");
        assert_eq!(body["policies"]["max_turns"], 4);
        let _ = std::fs::remove_file(&path);
    }

    #[test]
    fn load_config_body_keeps_undocumented_keys() {
        let path = temp_json(r#"{"name":"A","invented_later":{"deep":[1,2]}}"#);
        let body = load_config_body(Some(&path)).unwrap();
        assert_eq!(body["invented_later"]["deep"][1], 2);
        let _ = std::fs::remove_file(&path);
    }

    #[test]
    fn load_config_body_rejects_a_missing_file() {
        let path = std::env::temp_dir().join("memorylake-agent-body-does-not-exist.json");
        let err = format!("{:#}", load_config_body(Some(&path)).unwrap_err());
        assert!(err.contains("read config file"), "got: {err}");
    }

    #[test]
    fn load_config_body_rejects_malformed_json() {
        let path = temp_json("{not json");
        let err = format!("{:#}", load_config_body(Some(&path)).unwrap_err());
        assert!(err.contains("parse JSON from config file"), "got: {err}");
        let _ = std::fs::remove_file(&path);
    }

    #[test]
    fn load_config_body_rejects_a_non_object_top_level() {
        let path = temp_json(r#"["a","b"]"#);
        let err = format!("{:#}", load_config_body(Some(&path)).unwrap_err());
        assert!(err.contains("must hold a JSON object"), "got: {err}");
        assert!(err.contains("an array"), "got: {err}");
        let _ = std::fs::remove_file(&path);
    }

    #[test]
    fn scalar_flags_override_the_config_file() {
        let path = temp_json(r#"{"name":"FromFile","model":"from-file"}"#);
        let mut body = load_config_body(Some(&path)).unwrap();
        set_scalar(&mut body, "name", Some("FromFlag".into()));
        set_scalar(&mut body, "model", None);
        assert_eq!(body["name"], "FromFlag");
        assert_eq!(body["model"], "from-file", "an unset flag must not clobber");
        let _ = std::fs::remove_file(&path);
    }

    #[test]
    fn require_field_accepts_values_from_either_source() {
        let mut body = AgentRequestBody::new();
        assert!(require_field(&body, "name", "name").is_err());

        body.insert("name".into(), Value::String("  ".into()));
        assert!(
            require_field(&body, "name", "name").is_err(),
            "blank strings do not satisfy a required field"
        );

        body.insert("name".into(), Value::String("Support".into()));
        assert!(require_field(&body, "name", "name").is_ok());
    }

    #[test]
    fn require_field_error_names_the_flag_and_the_key() {
        let err = require_field(&AgentRequestBody::new(), "custom_id", "custom-id")
            .unwrap_err()
            .to_string();
        assert!(err.contains("--custom-id"), "got: {err}");
        assert!(err.contains("custom_id"), "got: {err}");
    }

    #[test]
    fn reject_config_fields_allows_identity_only_bodies() {
        let mut body = AgentRequestBody::new();
        body.insert("name".into(), Value::String("A".into()));
        body.insert("description".into(), Value::String("d".into()));
        body.insert("metadata".into(), serde_json::json!({"team": "core"}));
        assert!(reject_config_fields(&body).is_ok());
    }

    #[test]
    fn reject_config_fields_catches_keys_from_config_files() {
        let mut body = AgentRequestBody::new();
        body.insert("name".into(), Value::String("A".into()));
        body.insert("model".into(), Value::String("gpt-4o".into()));
        let err = reject_config_fields(&body).unwrap_err().to_string();
        assert!(err.contains("`model`"), "got: {err}");
        assert!(err.contains("agent version create"), "got: {err}");
    }

    #[test]
    fn reject_config_fields_lists_every_offender() {
        let mut body = AgentRequestBody::new();
        for field in ["policies", "subagents"] {
            body.insert(field.into(), serde_json::json!({}));
        }
        let err = reject_config_fields(&body).unwrap_err().to_string();
        assert!(err.contains("`policies`"), "got: {err}");
        assert!(err.contains("`subagents`"), "got: {err}");
    }

    #[test]
    fn reject_config_fields_lets_undocumented_keys_through() {
        let mut body = AgentRequestBody::new();
        body.insert("invented_later".into(), Value::Bool(true));
        assert!(
            reject_config_fields(&body).is_ok(),
            "the guardrail is best-effort against known config keys only"
        );
    }

    #[test]
    fn from_version_parses_latest_and_numbers() {
        assert_eq!(
            "latest".parse::<FromVersion>().unwrap(),
            FromVersion::Latest
        );
        assert_eq!(
            "LATEST".parse::<FromVersion>().unwrap(),
            FromVersion::Latest
        );
        assert_eq!(
            " 7 ".parse::<FromVersion>().unwrap(),
            FromVersion::Number(7)
        );
    }

    #[test]
    fn from_version_rejects_anything_else() {
        let err = "v3".parse::<FromVersion>().unwrap_err();
        assert!(
            err.contains("expected `latest` or a version number"),
            "got: {err}"
        );
        assert!("-1".parse::<FromVersion>().is_err());
    }
}
