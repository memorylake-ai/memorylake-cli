//! Update a project
//! (`PATCH /api/v3/workspaces/{workspace_id}/projects/{project_id}`).

use serde::Serialize;

use crate::client::Client;
use crate::error::Result;

use super::path::project_path;
use super::types::Project;

/// Request body for updating a project.
///
/// The endpoint applies partial-update semantics: only the keys present in the
/// body are modified. Every field is therefore skipped when `None`, because
/// sending `null` would be a different instruction than "leave this alone".
///
/// The endpoint also accepts `industry_ids`; it is not exposed yet, so it is
/// never sent.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize)]
pub struct UpdateProjectRequest {
    /// New display name, when changing it.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,
    /// New description, when changing it.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
}

/// Update the mutable fields of a project.
///
/// An empty request sends `{}` and lets the server decide whether that is an
/// error or a no-op returning the unchanged project.
pub fn update_project(
    client: &Client,
    workspace_id: &str,
    project_id: &str,
    request: &UpdateProjectRequest,
) -> Result<Project> {
    client.patch_data(&project_path(workspace_id, project_id), request)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn omitted_fields_are_absent_from_the_body() {
        let body = serde_json::to_string(&UpdateProjectRequest {
            name: Some("New Name".into()),
            description: None,
        })
        .expect("serialize");
        assert_eq!(body, r#"{"name":"New Name"}"#);
    }

    #[test]
    fn empty_request_serializes_to_an_empty_object() {
        let body = serde_json::to_string(&UpdateProjectRequest::default()).expect("serialize");
        assert_eq!(body, "{}");
    }

    #[test]
    fn explicit_empty_string_is_sent_verbatim() {
        // `--description ""` is a real instruction; the server decides what it
        // means. Only an absent flag is "leave this alone".
        let body = serde_json::to_string(&UpdateProjectRequest {
            name: None,
            description: Some(String::new()),
        })
        .expect("serialize");
        assert_eq!(body, r#"{"description":""}"#);
    }
}
