//! Shared skill resource types.
//!
//! Response structs capture the documented fields and collect anything else the
//! server sends into `extra` via `#[serde(flatten)]`, so re-serializing a
//! response never silently drops data the CLI did not know about.

use serde::{Deserialize, Serialize};
use serde_json::{Map, Value};

/// Reference to a package already uploaded through
/// [`super::create_package_upload`].
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct PackageRef {
    /// Storage URI exactly as the upload slot returned it.
    ///
    /// Opaque: its scheme depends on the storage behind the deployment, and
    /// only URIs issued by the upload endpoint are accepted.
    pub s3_uri: String,
}

/// A skill: a named, versioned package agents can be configured to use.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Skill {
    /// Server-assigned skill id. Refer to a skill by this.
    pub id: String,
    /// Stable identifier for the skill.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,
    /// What the skill does.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    /// Title shown in the console.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub display_title: Option<String>,
    /// Latest published version number.
    ///
    /// Zero means nothing has been published yet; absent means the current
    /// state could not be read.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub latest_version: Option<u64>,
    /// Security review state of the latest version: `pending`, `safe`,
    /// `blocked`, or `error`.
    ///
    /// Absent with `latest_version` above zero means the deployment has the
    /// review switched off and the skill is usable; absent with
    /// `latest_version` zero means no package is published and it is not.
    /// Kept as a string so a state added later is shown rather than rejected.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub latest_security_status: Option<String>,
    /// Creation timestamp (ISO 8601).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub created_at: Option<String>,
    /// User or agent that created the skill.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub created_by: Option<String>,
    /// Last update timestamp (ISO 8601).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub updated_at: Option<String>,
    /// Fields returned by the server that this client does not model.
    #[serde(flatten)]
    pub extra: Map<String, Value>,
}

/// One published version of a skill.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SkillVersion {
    /// Version number, assigned on publish and never reused.
    pub version: u64,
    /// Skill this version belongs to.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub skill_id: Option<String>,
    /// Release notes. The first version's is server-generated
    /// (`Initial version`, measured 2026-10-09).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub changelog: Option<String>,
    /// Security review state of this version: `pending`, `safe`, `blocked`,
    /// or `error`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub security_status: Option<String>,
    /// Publication timestamp (ISO 8601).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub created_at: Option<String>,
    /// Fields returned by the server that this client does not model.
    #[serde(flatten)]
    pub extra: Map<String, Value>,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn skill_round_trip_preserves_unmodeled_fields() {
        let raw = serde_json::json!({
            "id": "skill-1",
            "name": "equity-research",
            "display_title": "Equity Research",
            "latest_version": 2,
            "latest_security_status": "safe",
            "future_field": {"nested": true}
        });
        let skill: Skill = serde_json::from_value(raw.clone()).expect("deserialize skill");
        assert_eq!(skill.latest_version, Some(2));
        assert_eq!(skill.latest_security_status.as_deref(), Some("safe"));
        assert_eq!(serde_json::to_value(&skill).unwrap(), raw);
    }

    #[test]
    fn skill_accepts_a_minimal_body_and_an_unknown_review_state() {
        let skill: Skill =
            serde_json::from_value(serde_json::json!({"id": "skill-1"})).expect("minimal");
        assert!(skill.latest_version.is_none());

        let skill: Skill = serde_json::from_value(serde_json::json!({
            "id": "skill-1",
            "latest_security_status": "quarantined"
        }))
        .expect("an unknown review state is not a decode error");
        assert_eq!(skill.latest_security_status.as_deref(), Some("quarantined"));
    }

    #[test]
    fn version_round_trip_preserves_unmodeled_fields() {
        let raw = serde_json::json!({
            "version": 1,
            "changelog": "Initial version",
            "skill_id": "skill-1",
            "security_status": "pending",
            "reviewer_notes": "keep me"
        });
        let version: SkillVersion = serde_json::from_value(raw.clone()).unwrap();
        assert_eq!(version.version, 1);
        assert_eq!(serde_json::to_value(&version).unwrap(), raw);
    }

    #[test]
    fn package_ref_serializes_the_documented_shape() {
        let body = serde_json::to_value(PackageRef {
            s3_uri: "s3://bucket/tmp/x.zip".into(),
        })
        .unwrap();
        assert_eq!(body, serde_json::json!({"s3_uri": "s3://bucket/tmp/x.zip"}));
    }
}
