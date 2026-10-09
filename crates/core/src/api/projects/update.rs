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
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize)]
pub struct UpdateProjectRequest {
    /// New display name, when changing it.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,
    /// New description, when changing it.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    /// Replacement industry opendata ids. **Replaces** the attached set rather
    /// than merging into it; `Some(vec![])` detaches every industry, and `None`
    /// leaves them as they are (measured against production 2026-10-09).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub industry_ids: Option<Vec<String>>,
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
            ..UpdateProjectRequest::default()
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
            description: Some(String::new()),
            ..UpdateProjectRequest::default()
        })
        .expect("serialize");
        assert_eq!(body, r#"{"description":""}"#);
    }

    #[test]
    fn an_empty_industry_list_is_sent_because_it_detaches_every_industry() {
        let body = serde_json::to_string(&UpdateProjectRequest {
            industry_ids: Some(Vec::new()),
            ..UpdateProjectRequest::default()
        })
        .expect("serialize");
        assert_eq!(body, r#"{"industry_ids":[]}"#);
    }
}
