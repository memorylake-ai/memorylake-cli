//! Request shapes for the A2A v1.0 `SendMessage` operation.
//!
//! Field names are camelCase on the wire, as A2A spells them, unlike the
//! snake_case used by the rest of the MemoryLake API. Only the fields the CLI
//! sets are modelled; message parts are open JSON so the caller can send any
//! part kind the protocol defines.

use serde::Serialize;
use serde_json::{Map, Value};

/// The `role` of a message sent by the caller.
pub const ROLE_USER: &str = "ROLE_USER";

/// A task the agent is still working on.
pub const TASK_STATE_WORKING: &str = "TASK_STATE_WORKING";
/// A task that finished and produced its result.
pub const TASK_STATE_COMPLETED: &str = "TASK_STATE_COMPLETED";
/// A task that stopped with an error.
pub const TASK_STATE_FAILED: &str = "TASK_STATE_FAILED";
/// A task that was cancelled.
pub const TASK_STATE_CANCELED: &str = "TASK_STATE_CANCELED";
/// A task the agent refused.
pub const TASK_STATE_REJECTED: &str = "TASK_STATE_REJECTED";
/// A task waiting for the caller to say more; reply with its `taskId`.
pub const TASK_STATE_INPUT_REQUIRED: &str = "TASK_STATE_INPUT_REQUIRED";

/// Whether a task in `state` will change no further on its own.
///
/// `TASK_STATE_INPUT_REQUIRED` is *not* terminal: the task resumes when the
/// caller replies to it.
pub fn is_terminal_state(state: &str) -> bool {
    matches!(
        state,
        TASK_STATE_COMPLETED | TASK_STATE_FAILED | TASK_STATE_CANCELED | TASK_STATE_REJECTED
    )
}

/// A text part, the one part kind the CLI builds itself.
pub fn text_part(text: impl Into<String>) -> Value {
    let mut part = Map::new();
    part.insert("text".into(), Value::String(text.into()));
    Value::Object(part)
}

/// Body of `POST .../a2a/message:send` and `message:stream`.
#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SendMessageRequest {
    /// The message to deliver.
    pub message: Message,
    /// How the server should answer.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub configuration: Option<SendConfiguration>,
    /// Request metadata; MemoryLake reads its own extension from here.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub metadata: Option<SendMetadata>,
}

/// One A2A message.
#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Message {
    /// Who is speaking; the CLI always sends [`ROLE_USER`].
    pub role: String,
    /// Caller-chosen id, unique within the context.
    pub message_id: String,
    /// Content parts, e.g. `{"text": "..."}`.
    pub parts: Vec<Value>,
    /// Conversation thread to continue. Omitted starts a new one.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub context_id: Option<String>,
    /// Task to continue, for answering a `TASK_STATE_INPUT_REQUIRED` task.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub task_id: Option<String>,
}

/// `configuration` of a send request.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SendConfiguration {
    /// Answer as soon as the task exists instead of when it finishes.
    ///
    /// The server blocks by default (measured: a short answer returns
    /// `TASK_STATE_COMPLETED` with its artifacts in a few seconds); `true`
    /// returns the task in `TASK_STATE_WORKING` for the caller to poll.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub return_immediately: Option<bool>,
    /// How many history messages to include in the returned task.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub history_length: Option<u32>,
}

impl SendConfiguration {
    /// Whether nothing was set, so the whole object can be left out.
    pub fn is_empty(&self) -> bool {
        *self == Self::default()
    }
}

/// `metadata` of a send request.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct SendMetadata {
    /// MemoryLake's extension: which actor speaks, which projects are in
    /// scope, whether to remember the exchange.
    pub memorylake: MemorylakeExtension,
}

/// `metadata.memorylake` of a send request.
///
/// Everything is optional; the server falls back to the agent's own defaults
/// for anything left out. `extra` carries keys the CLI does not model
/// (`overrides`, `subagentMapping`, …) verbatim.
#[derive(Debug, Clone, Default, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct MemorylakeExtension {
    /// Actor the message is attributed to. Need not be bound to the
    /// workspace: production accepts the caller's own actor as is.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub actor_id: Option<String>,
    /// Project the agent may read from and write memories to.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub read_write_project_id: Option<String>,
    /// Further projects the agent may read but not write.
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub read_only_project_ids: Vec<String>,
    /// Do not extract memories from this exchange.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub skip_memory: Option<bool>,
    /// Unmodelled keys, merged into the object as given.
    #[serde(flatten)]
    pub extra: Map<String, Value>,
}

impl MemorylakeExtension {
    /// Whether nothing was set, so `metadata` can be left out entirely.
    pub fn is_empty(&self) -> bool {
        self.actor_id.is_none()
            && self.read_write_project_id.is_none()
            && self.read_only_project_ids.is_empty()
            && self.skip_memory.is_none()
            && self.extra.is_empty()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn minimal() -> SendMessageRequest {
        SendMessageRequest {
            message: Message {
                role: ROLE_USER.into(),
                message_id: "m-1".into(),
                parts: vec![text_part("hi")],
                context_id: None,
                task_id: None,
            },
            configuration: None,
            metadata: None,
        }
    }

    #[test]
    fn a_minimal_request_sends_only_the_message() {
        assert_eq!(
            serde_json::to_value(minimal()).unwrap(),
            json!({"message": {"role": "ROLE_USER", "messageId": "m-1", "parts": [{"text": "hi"}]}})
        );
    }

    #[test]
    fn field_names_are_camel_case_on_the_wire() {
        let mut request = minimal();
        request.message.context_id = Some("ctx".into());
        request.message.task_id = Some("run-1".into());
        request.configuration = Some(SendConfiguration {
            return_immediately: Some(true),
            history_length: Some(3),
        });
        request.metadata = Some(SendMetadata {
            memorylake: MemorylakeExtension {
                actor_id: Some("act-1".into()),
                read_write_project_id: Some("proj-1".into()),
                read_only_project_ids: vec!["proj-2".into()],
                skip_memory: Some(true),
                extra: Map::new(),
            },
        });
        assert_eq!(
            serde_json::to_value(request).unwrap(),
            json!({
                "message": {
                    "role": "ROLE_USER", "messageId": "m-1", "parts": [{"text": "hi"}],
                    "contextId": "ctx", "taskId": "run-1"
                },
                "configuration": {"returnImmediately": true, "historyLength": 3},
                "metadata": {"memorylake": {
                    "actorId": "act-1",
                    "readWriteProjectId": "proj-1",
                    "readOnlyProjectIds": ["proj-2"],
                    "skipMemory": true
                }}
            })
        );
    }

    #[test]
    fn extra_extension_keys_are_flattened_into_the_object() {
        let mut extra = Map::new();
        extra.insert("overrides".into(), json!({"model": "x"}));
        let ext = MemorylakeExtension {
            extra,
            ..Default::default()
        };
        assert!(!ext.is_empty());
        assert_eq!(
            serde_json::to_value(ext).unwrap(),
            json!({"overrides": {"model": "x"}})
        );
    }

    #[test]
    fn empty_extension_and_configuration_report_so() {
        assert!(MemorylakeExtension::default().is_empty());
        assert!(SendConfiguration::default().is_empty());
        assert!(
            !SendConfiguration {
                return_immediately: Some(false),
                ..Default::default()
            }
            .is_empty(),
            "an explicit false is still a setting"
        );
    }

    #[test]
    fn only_finished_states_are_terminal() {
        for state in [
            TASK_STATE_COMPLETED,
            TASK_STATE_FAILED,
            TASK_STATE_CANCELED,
            TASK_STATE_REJECTED,
        ] {
            assert!(is_terminal_state(state), "{state}");
        }
        assert!(!is_terminal_state(TASK_STATE_WORKING));
        assert!(!is_terminal_state(TASK_STATE_INPUT_REQUIRED));
        assert!(!is_terminal_state("TASK_STATE_UNSPECIFIED"));
    }
}
