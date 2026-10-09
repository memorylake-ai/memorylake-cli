//! Draft an instruction for a database memory
//! (`POST .../memories/databases/{database_id}/instruction/generate`).

use serde::{Deserialize, Serialize};
use serde_json::{Map, Value};

use crate::client::Client;
use crate::error::{Error, Result};

use super::path::generate_instruction_path;

/// A drafted instruction. It has not been saved.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct InstructionDraft {
    /// The drafted text. To keep it, send it as `instruction` in
    /// [`super::update_database_memory`].
    #[serde(default)]
    pub instruction: String,
    /// Fields returned by the server that this client does not model.
    #[serde(flatten)]
    pub extra: Map<String, Value>,
}

/// Ask the server to draft an instruction for a database memory.
///
/// Always uses the endpoint's streaming mode. Drafting is model work that can
/// outlast the HTTP client's 30-second timeout for a response to begin; the
/// stream starts at once and keeps the connection alive with comments until
/// the single `data` event carrying the whole draft. The result is the same
/// draft the non-streaming mode returns.
///
/// Errors before the stream opens (an unknown id, for example) arrive as an
/// ordinary envelope; one after it opens arrives as an `error` event carrying
/// `error_code`, and is returned as [`Error::Api`] with that code.
///
/// The event framing follows the published description; no database memory
/// existed on production to measure it against (2026-10-09), so frames are
/// read leniently: a draft is taken from the first frame carrying an
/// `instruction`, at the top level or inside an envelope's `data`.
pub fn generate_database_instruction(
    client: &Client,
    workspace_id: &str,
    project_id: &str,
    database_id: &str,
) -> Result<InstructionDraft> {
    let events = client.post_event_stream(
        &generate_instruction_path(workspace_id, project_id, database_id),
        &serde_json::json!({"stream": true}),
        &[],
    )?;
    draft_from_events(events)
}

/// Pick the draft, or the failure, out of the stream's frames.
///
/// Stops at the first frame that settles the outcome, so the trailing `end`
/// event is never read and its payload cannot matter.
fn draft_from_events(events: impl IntoIterator<Item = Result<Value>>) -> Result<InstructionDraft> {
    for event in events {
        let frame = event?;
        if let Some(error) = error_from_frame(&frame) {
            return Err(error);
        }
        let payload = match frame.get("data") {
            Some(data @ Value::Object(_)) => data,
            _ => &frame,
        };
        if payload.get("instruction").is_some() {
            return serde_json::from_value(payload.clone()).map_err(|source| Error::Api {
                message: format!("instruction draft has an unexpected shape: {source}\n{frame}"),
                code: None,
            });
        }
    }
    Err(Error::Api {
        message: "instruction stream ended without a draft".into(),
        code: None,
    })
}

/// An `error` event, or a failed envelope sent as a frame.
fn error_from_frame(frame: &Value) -> Option<Error> {
    let code = frame
        .get("error_code")
        .and_then(Value::as_str)
        .filter(|code| !code.trim().is_empty())
        .map(str::to_string);
    let failed = frame.get("success") == Some(&Value::Bool(false));
    if code.is_none() && !failed {
        return None;
    }

    let message = frame
        .get("message")
        .and_then(Value::as_str)
        .filter(|message| !message.trim().is_empty())
        .unwrap_or("instruction generation failed");
    let suffix = code
        .as_deref()
        .map(|c| format!(" [{c}]"))
        .unwrap_or_default();
    Some(Error::Api {
        message: format!("{message}{suffix}"),
        code,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    use std::io::Cursor;

    use crate::sse::EventStream;

    fn draft_from_body(body: &str) -> Result<InstructionDraft> {
        draft_from_events(EventStream::new(Cursor::new(body.as_bytes().to_vec())))
    }

    #[test]
    fn the_documented_stream_yields_the_draft() {
        // Keepalive comments, one `data` event, then `end`.
        let body = ": keepalive\n\n: keepalive\n\n\
                    event: data\ndata: {\"instruction\":\"Amounts are in cents.\"}\n\n\
                    event: end\ndata: {}\n\n";
        let draft = draft_from_body(body).expect("draft");
        assert_eq!(draft.instruction, "Amounts are in cents.");
    }

    #[test]
    fn the_end_event_is_never_parsed() {
        // Whatever `end` carries, the draft has already been taken.
        let body = "event: data\ndata: {\"instruction\":\"x\"}\n\nevent: end\ndata: [DONE]\n\n";
        assert_eq!(draft_from_body(body).expect("draft").instruction, "x");
    }

    #[test]
    fn a_draft_inside_an_envelope_is_accepted() {
        let body = "data: {\"success\":true,\"data\":{\"instruction\":\"y\"}}\n\n";
        assert_eq!(draft_from_body(body).expect("draft").instruction, "y");
    }

    #[test]
    fn an_error_event_carries_its_code() {
        let body = ": keepalive\n\nevent: error\ndata: {\"error_code\":\"LLM_UNAVAILABLE\",\"message\":\"model busy\"}\n\n";
        match draft_from_body(body) {
            Err(Error::Api { message, code }) => {
                assert_eq!(code.as_deref(), Some("LLM_UNAVAILABLE"));
                assert!(message.contains("model busy"), "{message}");
                assert!(message.contains("[LLM_UNAVAILABLE]"), "{message}");
            }
            other => panic!("expected an API error, got {other:?}"),
        }
    }

    #[test]
    fn a_stream_without_a_draft_is_an_error() {
        let err = draft_from_body(": keepalive\n\nevent: end\ndata: {}\n\n")
            .expect_err("no draft must not pass for an empty one");
        assert!(err.to_string().contains("without a draft"), "{err}");
    }
}
