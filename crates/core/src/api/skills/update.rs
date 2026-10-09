//! Update a skill's metadata (`PATCH /api/v3/skills/{id}`).

use serde::Serialize;

use crate::client::Client;
use crate::error::Result;

use super::skill_path;
use super::types::Skill;

/// Request body for updating a skill.
///
/// Only supplied fields change, so each is skipped when `None`. The package is
/// not editable here — publish a version with
/// [`super::create_skill_version`] instead.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize)]
pub struct UpdateSkillRequest {
    /// New title shown in the console (at most 500 characters).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub display_title: Option<String>,
    /// New description. An empty string clears it (measured 2026-10-09).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
}

/// Change a skill's title and/or description. Built-in skills are refused.
pub fn update_skill(client: &Client, id: &str, request: &UpdateSkillRequest) -> Result<Skill> {
    client.patch_data(&skill_path(id), request)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn omitted_fields_are_absent_from_the_body() {
        let body = serde_json::to_string(&UpdateSkillRequest {
            display_title: Some("New".into()),
            description: None,
        })
        .unwrap();
        assert_eq!(body, r#"{"display_title":"New"}"#);
    }

    #[test]
    fn explicit_empty_description_is_sent_verbatim() {
        let body = serde_json::to_string(&UpdateSkillRequest {
            display_title: None,
            description: Some(String::new()),
        })
        .unwrap();
        assert_eq!(body, r#"{"description":""}"#);
    }
}
