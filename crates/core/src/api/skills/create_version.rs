//! Publish a skill version (`POST /api/v3/skills/{id}/versions`).

use serde::Serialize;

use crate::client::Client;
use crate::error::Result;

use super::skill_versions_path;
use super::types::{PackageRef, SkillVersion};

/// Request body for publishing a new version of a skill.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct CreateSkillVersionRequest {
    /// Release notes for this version.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub changelog: Option<String>,
    /// The uploaded package that replaces the skill's current one.
    pub package_ref: PackageRef,
}

/// Publish a new version of a skill, replacing its package.
///
/// The new version must pass security review, so references that follow the
/// skill's latest version cannot use it until then; agent versions pinned to
/// an older version number are unaffected.
pub fn create_skill_version(
    client: &Client,
    id: &str,
    request: &CreateSkillVersionRequest,
) -> Result<SkillVersion> {
    client.post_data(&skill_versions_path(id), request)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn body_has_the_documented_shape() {
        let body = serde_json::to_value(CreateSkillVersionRequest {
            changelog: Some("v2".into()),
            package_ref: PackageRef {
                s3_uri: "s3://b/k.zip".into(),
            },
        })
        .unwrap();
        assert_eq!(
            body,
            serde_json::json!({"changelog": "v2", "package_ref": {"s3_uri": "s3://b/k.zip"}})
        );
    }

    #[test]
    fn omitted_changelog_is_absent() {
        let body = serde_json::to_value(CreateSkillVersionRequest {
            changelog: None,
            package_ref: PackageRef { s3_uri: "u".into() },
        })
        .unwrap();
        assert_eq!(body, serde_json::json!({"package_ref": {"s3_uri": "u"}}));
    }
}
