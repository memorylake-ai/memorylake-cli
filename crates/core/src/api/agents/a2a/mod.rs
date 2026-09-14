//! Talking to a bound agent over A2A
//! (`/api/v3/workspaces/{workspace}/agents/{agent}/a2a/...`).
//!
//! MemoryLake exposes every agent bound to a workspace as an A2A server. This
//! module binds the **A2A v1.0 HTTP+JSON** surface only: the JSON-RPC endpoint
//! is the same operations behind one URL, and the v0.3 REST surface is a
//! superseded protocol revision.
//!
//! Mind the paths: the v1.0 REST operations live on *unversioned* paths
//! (`/a2a/message:send`, `/a2a/tasks`), while the paths carrying `/v1/`
//! (`/a2a/v1/message:send`) belong to protocol version 0.3. The agent card's
//! `supportedInterfaces` says so explicitly; the constants here are named by
//! protocol version, not by path.
//!
//! Responses are A2A-shaped, not MemoryLake envelopes, and the API documents
//! them without a schema, so they come back as [`serde_json::Value`]. Errors
//! do arrive as envelopes and are decoded like every other API error, then
//! run through [`describe_a2a_error`] to surface the A2A reason.

mod card;
mod error;
mod send;
mod tasks;
mod types;

pub use card::get_agent_card;
pub use error::describe_a2a_error;
pub use send::{send_message, stream_message};
pub use tasks::{
    FEEDBACK_COMMENT_MAX_CHARS, ListTasksParams, Rating, TaskFeedbackRequest, cancel_task,
    get_task, list_tasks, submit_task_feedback,
};
pub use types::{
    MemorylakeExtension, Message, ROLE_USER, SendConfiguration, SendMessageRequest, SendMetadata,
    TASK_STATE_CANCELED, TASK_STATE_COMPLETED, TASK_STATE_FAILED, TASK_STATE_INPUT_REQUIRED,
    TASK_STATE_REJECTED, TASK_STATE_WORKING, is_terminal_state, text_part,
};

use crate::api::path::encode_segment;

use super::workspace_agent_path;

/// Header that activates A2A extensions for a request.
///
/// Required to read task feedback back: without it `GET tasks/{id}` omits the
/// `metadata` the rating lives in. Submitting feedback works either way, so
/// the header is sent on both to keep the two symmetric.
pub const A2A_EXTENSIONS_HEADER: &str = "A2A-Extensions";

/// URI of MemoryLake's task-feedback extension, as advertised in the agent
/// card's `capabilities.extensions`.
pub const TASK_FEEDBACK_EXTENSION_URI: &str = "extensions://task-feedback/v1";

/// `/api/v3/workspaces/{ws}/agents/{agent}/a2a`
fn a2a_root(workspace_id: &str, agent_id: &str) -> String {
    format!("{}/a2a", workspace_agent_path(workspace_id, agent_id))
}

/// `/api/v3/workspaces/{ws}/agents/{agent}/.well-known/agent-card.json`
fn agent_card_path(workspace_id: &str, agent_id: &str) -> String {
    format!(
        "{}/.well-known/agent-card.json",
        workspace_agent_path(workspace_id, agent_id)
    )
}

/// `.../a2a/message:send` (A2A v1.0)
fn message_send_path(workspace_id: &str, agent_id: &str) -> String {
    format!("{}/message:send", a2a_root(workspace_id, agent_id))
}

/// `.../a2a/message:stream` (A2A v1.0)
fn message_stream_path(workspace_id: &str, agent_id: &str) -> String {
    format!("{}/message:stream", a2a_root(workspace_id, agent_id))
}

/// `.../a2a/tasks` (A2A v1.0)
fn tasks_path(workspace_id: &str, agent_id: &str) -> String {
    format!("{}/tasks", a2a_root(workspace_id, agent_id))
}

/// `.../a2a/tasks/{task}` (A2A v1.0)
fn task_path(workspace_id: &str, agent_id: &str, task_id: &str) -> String {
    format!(
        "{}/{}",
        tasks_path(workspace_id, agent_id),
        encode_segment(task_id)
    )
}

/// `.../a2a/tasks/{task}:cancel` (A2A v1.0)
fn task_cancel_path(workspace_id: &str, agent_id: &str, task_id: &str) -> String {
    format!("{}:cancel", task_path(workspace_id, agent_id, task_id))
}

/// `.../a2a/tasks/{task}:feedback` (MemoryLake extension, A2A v1.0 only)
fn task_feedback_path(workspace_id: &str, agent_id: &str, task_id: &str) -> String {
    format!("{}:feedback", task_path(workspace_id, agent_id, task_id))
}

/// Headers that turn the feedback extension on for a request.
fn feedback_extension_headers() -> [(&'static str, &'static str); 1] {
    [(A2A_EXTENSIONS_HEADER, TASK_FEEDBACK_EXTENSION_URI)]
}

#[cfg(test)]
mod tests {
    use super::*;

    const ROOT: &str = "/api/v3/workspaces/ws-1/agents/agt-1";

    #[test]
    fn v1_paths_carry_no_version_segment() {
        // The `/v1/` paths are protocol 0.3; v1.0 is the unversioned set.
        assert_eq!(
            message_send_path("ws-1", "agt-1"),
            format!("{ROOT}/a2a/message:send")
        );
        assert_eq!(
            message_stream_path("ws-1", "agt-1"),
            format!("{ROOT}/a2a/message:stream")
        );
        assert_eq!(tasks_path("ws-1", "agt-1"), format!("{ROOT}/a2a/tasks"));
        assert_eq!(
            task_path("ws-1", "agt-1", "run-1"),
            format!("{ROOT}/a2a/tasks/run-1")
        );
        assert_eq!(
            task_cancel_path("ws-1", "agt-1", "run-1"),
            format!("{ROOT}/a2a/tasks/run-1:cancel")
        );
        assert_eq!(
            task_feedback_path("ws-1", "agt-1", "run-1"),
            format!("{ROOT}/a2a/tasks/run-1:feedback")
        );
    }

    #[test]
    fn agent_card_lives_under_well_known() {
        assert_eq!(
            agent_card_path("ws-1", "agt-1"),
            format!("{ROOT}/.well-known/agent-card.json")
        );
    }

    #[test]
    fn a_task_id_cannot_escape_its_segment() {
        assert_eq!(
            task_path("ws-1", "agt-1", "run-1/../x?y"),
            format!("{ROOT}/a2a/tasks/run-1%2F..%2Fx%3Fy")
        );
    }
}
