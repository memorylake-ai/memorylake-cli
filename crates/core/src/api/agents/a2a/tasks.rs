//! A2A task operations (`.../a2a/tasks`, A2A v1.0).

use serde::Serialize;
use serde_json::Value;

use crate::client::Client;
use crate::error::{Error, Result};

use super::{
    describe_a2a_error, feedback_extension_headers, task_cancel_path, task_feedback_path,
    task_path, tasks_path,
};

/// Longest `comment` the feedback extension accepts, in characters.
pub const FEEDBACK_COMMENT_MAX_CHARS: usize = 2000;

/// Query parameters of `GET .../a2a/tasks`.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct ListTasksParams {
    /// Only tasks in this conversation thread.
    pub context_id: Option<String>,
    /// Only tasks in this state (`TASK_STATE_...`).
    pub status: Option<String>,
    /// Tasks per page, 1–100.
    pub page_size: Option<u32>,
    /// `nextPageToken` from the previous page.
    pub page_token: Option<String>,
    /// How many history messages to include per task.
    pub history_length: Option<u32>,
    /// Only tasks whose status changed after this ISO 8601 instant.
    pub status_timestamp_after: Option<String>,
    /// Include each task's artifacts.
    pub include_artifacts: Option<bool>,
}

impl ListTasksParams {
    /// Query pairs, camelCase as A2A spells them.
    pub fn to_query(&self) -> Vec<(&'static str, String)> {
        let mut query = Vec::new();
        if let Some(value) = &self.context_id {
            query.push(("contextId", value.clone()));
        }
        if let Some(value) = &self.status {
            query.push(("status", value.clone()));
        }
        if let Some(value) = self.page_size {
            query.push(("pageSize", value.to_string()));
        }
        if let Some(value) = &self.page_token {
            query.push(("pageToken", value.clone()));
        }
        if let Some(value) = self.history_length {
            query.push(("historyLength", value.to_string()));
        }
        if let Some(value) = &self.status_timestamp_after {
            query.push(("statusTimestampAfter", value.clone()));
        }
        if let Some(value) = self.include_artifacts {
            query.push(("includeArtifacts", value.to_string()));
        }
        query
    }
}

/// A thumbs-up or thumbs-down on a task.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum Rating {
    /// The task's result was good.
    Up,
    /// The task's result was bad.
    Down,
}

impl Rating {
    /// Wire spelling.
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Up => "up",
            Self::Down => "down",
        }
    }
}

/// Body of `POST .../a2a/tasks/{task}:feedback`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct TaskFeedbackRequest {
    /// The rating.
    pub rating: Rating,
    /// Optional free text, at most [`FEEDBACK_COMMENT_MAX_CHARS`] characters.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub comment: Option<String>,
}

/// List an agent's tasks, newest first.
///
/// The response is `{"tasks": [...], "nextPageToken": "...", "pageSize": N,
/// "totalSize": N}`.
pub fn list_tasks(
    client: &Client,
    workspace_id: &str,
    agent_id: &str,
    params: &ListTasksParams,
) -> Result<Value> {
    client
        .get_json_with_headers(
            &tasks_path(workspace_id, agent_id),
            &params.to_query(),
            &feedback_extension_headers(),
        )
        .map_err(describe_a2a_error)
}

/// Fetch one task, with `history_length` history messages.
///
/// The feedback extension is activated so a rating left on the task shows up
/// in its `metadata`.
pub fn get_task(
    client: &Client,
    workspace_id: &str,
    agent_id: &str,
    task_id: &str,
    history_length: Option<u32>,
) -> Result<Value> {
    let query: Vec<(&str, String)> = history_length
        .map(|n| vec![("historyLength", n.to_string())])
        .unwrap_or_default();
    client
        .get_json_with_headers(
            &task_path(workspace_id, agent_id, task_id),
            &query,
            &feedback_extension_headers(),
        )
        .map_err(describe_a2a_error)
}

/// Cancel a task that is still running.
///
/// A task already in a terminal state is refused with reason
/// `TASK_NOT_CANCELABLE`.
pub fn cancel_task(
    client: &Client,
    workspace_id: &str,
    agent_id: &str,
    task_id: &str,
) -> Result<Value> {
    client
        .post_json_with_headers(
            &task_cancel_path(workspace_id, agent_id, task_id),
            &serde_json::json!({}),
            &[],
        )
        .map_err(describe_a2a_error)
}

/// Rate a task. Returns the task with the rating in its `metadata`.
pub fn submit_task_feedback(
    client: &Client,
    workspace_id: &str,
    agent_id: &str,
    task_id: &str,
    request: &TaskFeedbackRequest,
) -> Result<Value> {
    if let Some(comment) = &request.comment {
        let chars = comment.chars().count();
        if chars > FEEDBACK_COMMENT_MAX_CHARS {
            return Err(Error::Api {
                message: format!(
                    "feedback comment is {chars} characters; the limit is {FEEDBACK_COMMENT_MAX_CHARS}"
                ),
                code: None,
            });
        }
    }
    client
        .post_json_with_headers(
            &task_feedback_path(workspace_id, agent_id, task_id),
            request,
            &feedback_extension_headers(),
        )
        .map_err(describe_a2a_error)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::api::agents::a2a::{A2A_EXTENSIONS_HEADER, TASK_FEEDBACK_EXTENSION_URI};
    use crate::test_support::{json_ok, one_shot_server};

    fn header_value(head: &str, name: &str) -> Option<String> {
        let needle = format!("{}:", name.to_ascii_lowercase());
        head.lines().find_map(|line| {
            line.to_ascii_lowercase()
                .starts_with(&needle)
                .then(|| line[needle.len()..].trim().to_string())
        })
    }

    #[test]
    fn list_params_are_camel_case_and_only_set_ones_are_sent() {
        assert!(ListTasksParams::default().to_query().is_empty());
        let query = ListTasksParams {
            context_id: Some("ctx".into()),
            status: Some("TASK_STATE_WORKING".into()),
            page_size: Some(5),
            page_token: Some("tok".into()),
            history_length: Some(2),
            status_timestamp_after: Some("2026-09-01T00:00:00Z".into()),
            include_artifacts: Some(true),
        }
        .to_query();
        assert_eq!(
            query,
            vec![
                ("contextId", "ctx".to_string()),
                ("status", "TASK_STATE_WORKING".to_string()),
                ("pageSize", "5".to_string()),
                ("pageToken", "tok".to_string()),
                ("historyLength", "2".to_string()),
                ("statusTimestampAfter", "2026-09-01T00:00:00Z".to_string()),
                ("includeArtifacts", "true".to_string()),
            ]
        );
    }

    #[test]
    fn get_task_activates_the_feedback_extension() {
        let task = r#"{"id":"run-1","status":{"state":"TASK_STATE_COMPLETED"},"metadata":{"task-feedback/v1":{"rating":"up"}}}"#;
        let (base_url, server) = one_shot_server(json_ok(task));
        let client = Client::new(base_url, "sk-test").unwrap();

        let value = get_task(&client, "ws-1", "agt-1", "run-1", Some(3)).expect("task");
        assert_eq!(value["metadata"]["task-feedback/v1"]["rating"], "up");

        let captured = server.join().unwrap();
        assert!(
            captured.head.starts_with(
                "GET /api/v3/workspaces/ws-1/agents/agt-1/a2a/tasks/run-1?historyLength=3 "
            ),
            "{}",
            captured.head
        );
        assert_eq!(
            header_value(&captured.head, A2A_EXTENSIONS_HEADER).as_deref(),
            Some(TASK_FEEDBACK_EXTENSION_URI)
        );
    }

    #[test]
    fn feedback_posts_the_rating_with_the_extension_header() {
        let (base_url, server) = one_shot_server(json_ok(r#"{"id":"run-1"}"#));
        let client = Client::new(base_url, "sk-test").unwrap();

        submit_task_feedback(
            &client,
            "ws-1",
            "agt-1",
            "run-1",
            &TaskFeedbackRequest {
                rating: Rating::Down,
                comment: Some("meh".into()),
            },
        )
        .expect("feedback");

        let captured = server.join().unwrap();
        assert!(
            captured
                .head
                .starts_with("POST /api/v3/workspaces/ws-1/agents/agt-1/a2a/tasks/run-1:feedback "),
            "{}",
            captured.head
        );
        assert_eq!(
            header_value(&captured.head, A2A_EXTENSIONS_HEADER).as_deref(),
            Some(TASK_FEEDBACK_EXTENSION_URI)
        );
        let body: Value = serde_json::from_slice(&captured.body).unwrap();
        assert_eq!(
            body,
            serde_json::json!({"rating": "down", "comment": "meh"})
        );
    }

    #[test]
    fn an_over_long_comment_is_refused_before_any_request() {
        // Unreachable base URL: a request would fail with a connect error, not
        // with the message asserted here.
        let client = Client::new("http://127.0.0.1:1", "sk-test").unwrap();
        let err = submit_task_feedback(
            &client,
            "ws-1",
            "agt-1",
            "run-1",
            &TaskFeedbackRequest {
                rating: Rating::Up,
                comment: Some("x".repeat(FEEDBACK_COMMENT_MAX_CHARS + 1)),
            },
        )
        .expect_err("too long");
        assert!(err.to_string().contains("2001 characters"), "{err}");
    }

    #[test]
    fn cancel_of_a_finished_task_surfaces_the_a2a_reason() {
        // The production response, verbatim: an envelope whose `message` is
        // the A2A error document as a string.
        let body = r#"{"success":false,"message":"{\"error\":{\"code\":400,\"status\":\"FAILED_PRECONDITION\",\"message\":\"Task cannot be canceled - current state: 3\",\"details\":[{\"@type\":\"type.googleapis.com/google.rpc.ErrorInfo\",\"reason\":\"TASK_NOT_CANCELABLE\",\"domain\":\"a2a-protocol.org\",\"metadata\":{}}]}}","error_code":"INVALID_ARGUMENT"}"#;
        let (base_url, _server) = one_shot_server(format!(
            "HTTP/1.1 400 Bad Request\r\nContent-Type: application/json\r\nContent-Length: {}\r\n\r\n{body}",
            body.len()
        ));
        let client = Client::new(base_url, "sk-test").unwrap();

        let err = cancel_task(&client, "ws-1", "agt-1", "run-1").expect_err("not cancelable");
        let text = err.to_string();
        assert!(
            text.starts_with("Task cannot be canceled - current state: 3 (TASK_NOT_CANCELABLE)"),
            "{text}"
        );
        assert!(
            matches!(&err, Error::Api { code: Some(code), .. } if code == "INVALID_ARGUMENT"),
            "{err:?}"
        );
    }
}
