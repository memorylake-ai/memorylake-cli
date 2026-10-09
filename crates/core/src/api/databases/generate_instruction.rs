//! Draft an instruction for a database memory
//! (`POST .../memories/databases/{database_id}/instruction/generate`).

use std::io::BufRead;

use serde::{Deserialize, Serialize};
use serde_json::{Map, Value};

use crate::client::{Client, StreamOrData};
use crate::error::{Error, Result};
use crate::sse::EventStream;

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
/// Asks for the endpoint's streaming mode. Drafting is model work that can
/// outlast the HTTP client's 30-second timeout for a response to begin; the
/// stream starts at once and keeps the connection alive with comments until
/// the single `data` event carrying the whole draft. Should the server ignore
/// `stream` and answer with an ordinary JSON envelope, that is accepted too.
///
/// Errors before the stream opens (an unknown id, for example) arrive as an
/// ordinary envelope. After it opens, an `error` event is always a failure,
/// whatever its payload; it is returned as [`Error::Api`] carrying the
/// event's `error_code` when it has one and the server's raw event data in
/// the message.
///
/// The framing follows the published description; no database memory existed
/// on production to measure it against (2026-10-09), so data frames are read
/// leniently: the draft is taken from the first one carrying `instruction`, at
/// the top level or inside an envelope's `data`.
pub fn generate_database_instruction(
    client: &Client,
    workspace_id: &str,
    project_id: &str,
    database_id: &str,
) -> Result<InstructionDraft> {
    match client.post_event_stream_or_data(
        &generate_instruction_path(workspace_id, project_id, database_id),
        &serde_json::json!({"stream": true}),
        &[],
    )? {
        StreamOrData::Stream(mut stream) => draft_from_stream(&mut stream),
        StreamOrData::Data { data, .. } => draft_from_payload(&data)?.ok_or_else(|| Error::Api {
            message: format!("instruction response carried no draft\n{data}"),
            code: None,
        }),
    }
}

/// Pick the draft, or the failure, out of the stream's events.
///
/// Stops at the first event that settles the outcome, so nothing after the
/// draft is read.
fn draft_from_stream<R: BufRead>(stream: &mut EventStream<R>) -> Result<InstructionDraft> {
    while let Some(event) = stream.next_event()? {
        let data = event.data.unwrap_or_default();
        match event.event.as_deref() {
            Some("error") => return Err(error_event(&data)),
            Some("end") => break,
            _ => {}
        }
        if data.trim().is_empty() {
            continue;
        }

        let frame: Value = serde_json::from_str(&data).map_err(|source| Error::Api {
            message: format!("instruction stream sent a frame that is not JSON: {source}\n{data}"),
            code: None,
        })?;
        if frame.get("success") == Some(&Value::Bool(false)) || frame.get("error_code").is_some() {
            return Err(error_event(&data));
        }
        if let Some(draft) = draft_from_payload(&frame)? {
            return Ok(draft);
        }
    }
    Err(Error::Api {
        message: "instruction stream ended without a draft".into(),
        code: None,
    })
}

/// The draft in `value`, or in its envelope `data`; `None` if neither has one.
fn draft_from_payload(value: &Value) -> Result<Option<InstructionDraft>> {
    let payload = match value.get("data") {
        Some(data @ Value::Object(_)) => data,
        _ => value,
    };
    if payload.get("instruction").is_none() {
        return Ok(None);
    }
    serde_json::from_value(payload.clone())
        .map(Some)
        .map_err(|source| Error::Api {
            message: format!("instruction draft has an unexpected shape: {source}\n{value}"),
            code: None,
        })
}

/// An error event, keeping the server's raw data in the message.
///
/// `error_code` is lifted out when the data is JSON carrying one, so callers
/// can match on it; the data itself is always shown, parsed or not.
fn error_event(raw: &str) -> Error {
    let code = serde_json::from_str::<Value>(raw)
        .ok()
        .and_then(|frame| frame.get("error_code")?.as_str().map(str::to_string))
        .filter(|code| !code.trim().is_empty());
    let suffix = code
        .as_deref()
        .map(|c| format!(" [{c}]"))
        .unwrap_or_default();
    let raw = raw.trim();
    let detail = if raw.is_empty() {
        "(no event data)"
    } else {
        raw
    };
    Error::Api {
        message: format!("instruction generation failed{suffix}\n{detail}"),
        code,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    use std::io::Cursor;

    fn draft_from_body(body: &str) -> Result<InstructionDraft> {
        draft_from_stream(&mut EventStream::new(Cursor::new(body.as_bytes().to_vec())))
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
    fn nothing_after_the_draft_is_parsed() {
        let body = "event: data\ndata: {\"instruction\":\"x\"}\n\nevent: end\ndata: [DONE]\n\n";
        assert_eq!(draft_from_body(body).expect("draft").instruction, "x");
    }

    #[test]
    fn a_draft_inside_an_envelope_is_accepted() {
        let body = "data: {\"success\":true,\"data\":{\"instruction\":\"y\"}}\n\n";
        assert_eq!(draft_from_body(body).expect("draft").instruction, "y");
    }

    #[test]
    fn an_error_event_carries_its_code_and_the_raw_message() {
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
    fn an_error_event_is_a_failure_whatever_it_carries() {
        // Not JSON, no code, even an `instruction` key: still a failure, and
        // the server's words are kept.
        for (body, needle) in [
            (
                "event: error\ndata: upstream timed out\n\n",
                "upstream timed out",
            ),
            ("event: error\n\n", "no event data"),
            (
                "event: error\ndata: {\"instruction\":\"partial\"}\n\n",
                "partial",
            ),
        ] {
            let err = draft_from_body(body).expect_err(body);
            assert!(err.to_string().contains(needle), "{body}: {err}");
        }
    }

    #[test]
    fn an_end_before_any_draft_is_an_error() {
        let err = draft_from_body(": keepalive\n\nevent: end\ndata: {}\n\nevent: data\ndata: {\"instruction\":\"late\"}\n\n")
            .expect_err("no draft must not pass for an empty one");
        assert!(err.to_string().contains("without a draft"), "{err}");
    }

    #[test]
    fn an_envelope_payload_yields_its_draft() {
        // What the non-streaming mode returns once the envelope is unwrapped.
        let draft = draft_from_payload(&serde_json::json!({"instruction": "z"}))
            .expect("decode")
            .expect("present");
        assert_eq!(draft.instruction, "z");
        assert!(
            draft_from_payload(&serde_json::json!({}))
                .expect("decode")
                .is_none()
        );
    }
}
