//! Access scopes: per resource type, every instance or only listed ones.
//!
//! Members, roles, and access groups all carry one. A member's or role's
//! scope LIMITS what their role grants; an access group's only ever ADDS
//! instances. The wire shape is the same, so one set of types serves all
//! three.

use serde::{Deserialize, Serialize};
use serde_json::{Map, Value};

/// One resource type's scope as the server reports it.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AccessScopeEntry {
    /// Resource type, e.g. `memorylake:project`.
    pub resource_type: String,
    /// `all` (every instance) or `instances` (only the listed ones).
    pub mode: String,
    /// Read-only: the instance-level actions the role grants for this type —
    /// the most an entry can allow.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub available_actions: Option<Vec<String>>,
    /// The instances, when `mode` is `instances`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub instances: Option<Vec<Value>>,
    /// Fields the CLI does not model yet.
    #[serde(flatten)]
    pub extra: Map<String, Value>,
}

/// A member's access scope.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct MemberAccessScope {
    /// The member.
    pub principal_id: String,
    /// The member's primary role.
    #[serde(default)]
    pub role: Option<String>,
    /// One entry per scopable resource type.
    #[serde(default)]
    pub scopes: Vec<AccessScopeEntry>,
    /// Fields the CLI does not model yet.
    #[serde(flatten)]
    pub extra: Map<String, Value>,
}

/// A role's access scope, applied to everyone holding the role.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RoleAccessScope {
    /// The role.
    pub role_key: String,
    /// One entry per scopable resource type.
    #[serde(default)]
    pub scopes: Vec<AccessScopeEntry>,
    /// Fields the CLI does not model yet.
    #[serde(flatten)]
    pub extra: Map<String, Value>,
}

/// The instances an access group gives its members access to.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AccessGroupScope {
    /// The group.
    pub group_id: String,
    /// One entry per scopable resource type.
    #[serde(default)]
    pub scopes: Vec<AccessScopeEntry>,
    /// Fields the CLI does not model yet.
    #[serde(flatten)]
    pub extra: Map<String, Value>,
}

/// One resource type's scope as written.
///
/// Read-only fields of the reported shape ([`AccessScopeEntry`]) are
/// accepted and dropped, so the output of a `scope get` can be edited and
/// sent back as is. Anything else unknown is refused, so a misspelt field
/// fails locally instead of being ignored by the server.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ScopeEntryInput {
    /// Resource type, e.g. `memorylake:project`.
    pub resource_type: String,
    /// `all` or `instances`.
    pub mode: String,
    /// Required when `mode` is `instances`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub instances: Option<Vec<ScopeInstanceInput>>,
    /// Read-only; accepted for round-tripping and never sent.
    #[serde(default, skip_serializing)]
    pub available_actions: Option<Value>,
}

/// One instance of a [`ScopeEntryInput`].
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ScopeInstanceInput {
    /// Instance id, as the data API returns it.
    pub id: String,
    /// Parent workspace — required for project instances.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub workspace_id: Option<String>,
    /// Allowed actions; omit for every action the role allows.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub actions: Option<Vec<String>>,
    /// Echo of a read-only row: instances marked true are skipped by the
    /// server, since permission levels are set on the resource itself.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub via_permission_level: Option<bool>,
    /// Read-only; accepted for round-tripping and never sent.
    #[serde(default, skip_serializing)]
    pub effective_actions: Option<Value>,
    /// Read-only; accepted for round-tripping and never sent.
    #[serde(default, skip_serializing)]
    pub shadowed_actions: Option<Value>,
    /// Read-only; accepted for round-tripping and never sent.
    #[serde(default, skip_serializing)]
    pub permission_level: Option<Value>,
}

/// Request body replacing the scope of each listed resource type. Types not
/// listed keep their current scope.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct SetScopesRequest {
    /// Why the change is made; recorded in the audit log.
    pub reason: String,
    /// The replacement entries.
    pub scopes: Vec<ScopeEntryInput>,
}

/// Why a scope list cannot be sent.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
#[error("{0}")]
pub struct ScopeInputError(String);

/// Parse scope entries from JSON: either an array of entries, or an object
/// carrying them under `scopes` (the shape every `scope get` prints).
///
/// Checks what the server would otherwise refuse after a round trip: an
/// unknown mode, instances under `all`, `instances` with none listed, a
/// project instance without its workspace, and a resource type listed
/// twice.
///
/// An access group's `scope get` prints `instances` with none listed for a
/// type the group adds nothing to, yet writing that back is refused
/// (measured 2026-10-12); the error says to leave such entries out.
pub fn parse_scope_entries(json: &str) -> Result<Vec<ScopeEntryInput>, ScopeInputError> {
    let value: Value = serde_json::from_str(json)
        .map_err(|err| ScopeInputError(format!("scopes are not valid JSON: {err}")))?;
    let list = match value {
        Value::Array(_) => value,
        Value::Object(mut object) => object
            .remove("scopes")
            .ok_or_else(|| ScopeInputError("a scopes object must carry a `scopes` array".into()))?,
        _ => {
            return Err(ScopeInputError(
                "scopes must be a JSON array, or an object with a `scopes` array".into(),
            ));
        }
    };
    let entries: Vec<ScopeEntryInput> = serde_json::from_value(list)
        .map_err(|err| ScopeInputError(format!("invalid scope entry: {err}")))?;
    if entries.is_empty() {
        return Err(ScopeInputError("no scope entries given".into()));
    }
    let mut seen = std::collections::BTreeSet::new();
    for entry in &entries {
        let kind = &entry.resource_type;
        if !seen.insert(kind.as_str()) {
            return Err(ScopeInputError(format!("`{kind}` is listed twice")));
        }
        match (entry.mode.as_str(), &entry.instances) {
            ("all", Some(instances)) if !instances.is_empty() => {
                return Err(ScopeInputError(format!(
                    "`{kind}`: mode `all` takes no instances"
                )));
            }
            ("all", _) => {}
            ("instances", Some(instances)) if !instances.is_empty() => {
                if kind == "memorylake:project"
                    && let Some(bare) = instances.iter().find(|i| i.workspace_id.is_none())
                {
                    return Err(ScopeInputError(format!(
                        "`{kind}`: instance `{}` needs its `workspace_id`",
                        bare.id
                    )));
                }
            }
            ("instances", _) => {
                return Err(ScopeInputError(format!(
                    "`{kind}`: mode `instances` needs at least one instance. Leave the \
                     entry out to keep the type as it is; for an access group, mode `all` \
                     removes the type's instances"
                )));
            }
            (other, _) => {
                return Err(ScopeInputError(format!(
                    "`{kind}`: mode must be `all` or `instances`, not `{other}`"
                )));
            }
        }
    }
    Ok(entries)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_bare_array_and_a_get_output_both_parse() {
        let array = r#"[{"resource_type":"memorylake:workspace","mode":"all"}]"#;
        assert_eq!(parse_scope_entries(array).unwrap().len(), 1);

        // What `member scope get` prints, read-only fields and all.
        let printed = r#"{"principal_id":"p","role":"tenant_member","scopes":[
            {"resource_type":"memorylake:project","mode":"instances",
             "available_actions":["project:read"],
             "instances":[{"id":"proj-1","workspace_id":"ws-1","actions":["project:read"],
                           "effective_actions":["project:read"],"shadowed_actions":[],
                           "permission_level":"read","via_permission_level":false}]}]}"#;
        let entries = parse_scope_entries(printed).unwrap();
        let sent = serde_json::to_string(&entries).unwrap();
        assert_eq!(
            sent,
            r#"[{"resource_type":"memorylake:project","mode":"instances","instances":[{"id":"proj-1","workspace_id":"ws-1","actions":["project:read"],"via_permission_level":false}]}]"#,
            "read-only fields must not be sent back"
        );
    }

    #[test]
    fn what_the_server_would_refuse_is_refused_here() {
        for (json, needle) in [
            ("{}", "`scopes` array"),
            ("3", "JSON array"),
            ("[]", "no scope entries"),
            (
                r#"[{"resource_type":"t","mode":"all","bogus":1}]"#,
                "unknown field",
            ),
            (
                r#"[{"resource_type":"t","mode":"some"}]"#,
                "`all` or `instances`",
            ),
            (
                r#"[{"resource_type":"t","mode":"instances"}]"#,
                "at least one",
            ),
            (
                r#"[{"resource_type":"t","mode":"all","instances":[{"id":"a"}]}]"#,
                "takes no instances",
            ),
            (
                r#"[{"resource_type":"memorylake:project","mode":"instances","instances":[{"id":"p"}]}]"#,
                "workspace_id",
            ),
            (
                r#"[{"resource_type":"t","mode":"all"},{"resource_type":"t","mode":"all"}]"#,
                "listed twice",
            ),
        ] {
            let err = parse_scope_entries(json).unwrap_err().to_string();
            assert!(err.contains(needle), "{json}: {err}");
        }
    }
}
