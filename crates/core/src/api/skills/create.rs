//! Create a skill (`POST /api/v3/skills`).

use serde::Serialize;

use crate::client::Client;
use crate::error::Result;

use super::SKILLS_PATH;
use super::types::{PackageRef, Skill};

/// Request body for creating a skill.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct CreateSkillRequest {
    /// Stable identifier for the skill (at most 255 characters).
    ///
    /// The spec says names need not be unique, but production refuses a
    /// second skill with the same name in a tenant with `409
    /// SKILL_NAME_CONFLICT` (measured 2026-10-09).
    pub name: String,
    /// Title shown in the console (at most 500 characters).
    pub display_title: String,
    /// What the skill does.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    /// The uploaded package to publish as version 1.
    pub package_ref: PackageRef,
}

/// Create a skill from an already-uploaded package.
///
/// Returns immediately with the first version's review `pending`; poll
/// [`super::get_skill`] until `latest_security_status` settles. The server
/// rejects an archive that is not a ZIP, exceeds 10 MiB, or has no `SKILL.md`
/// at its root or one directory down, all as `INVALID_ARGUMENT`.
pub fn create_skill(client: &Client, request: &CreateSkillRequest) -> Result<Skill> {
    client.post_data(SKILLS_PATH, request)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn body_has_the_documented_shape() {
        let body = serde_json::to_value(CreateSkillRequest {
            name: "equity-research".into(),
            display_title: "Equity Research".into(),
            description: Some("Reads filings".into()),
            package_ref: PackageRef {
                s3_uri: "s3://b/k.zip".into(),
            },
        })
        .unwrap();
        assert_eq!(
            body,
            serde_json::json!({
                "name": "equity-research",
                "display_title": "Equity Research",
                "description": "Reads filings",
                "package_ref": {"s3_uri": "s3://b/k.zip"}
            })
        );
    }

    #[test]
    fn omitted_description_is_absent() {
        let body = serde_json::to_value(CreateSkillRequest {
            name: "n".into(),
            display_title: "t".into(),
            description: None,
            package_ref: PackageRef { s3_uri: "u".into() },
        })
        .unwrap();
        assert!(body.get("description").is_none(), "{body}");
    }
}
