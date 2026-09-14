//! Making A2A errors readable.
//!
//! An A2A failure reaches the client as a MemoryLake envelope whose `message`
//! is the A2A error document serialized *as a string*:
//!
//! ```json
//! {"success":false,"error_code":"NOT_FOUND",
//!  "message":"{\"error\":{\"code\":404,\"status\":\"NOT_FOUND\",\"message\":\"Task not found\",
//!             \"details\":[{\"reason\":\"TASK_NOT_FOUND\",\"domain\":\"a2a-protocol.org\"}]}}"}
//! ```
//!
//! The generic envelope decoder puts that string, escapes and all, at the top
//! of the error. This pass replaces it with the inner message and reason.

use serde::Deserialize;

use crate::error::Error;

#[derive(Debug, Deserialize)]
struct A2aErrorDocument {
    error: A2aErrorBody,
}

#[derive(Debug, Deserialize)]
struct A2aErrorBody {
    #[serde(default)]
    message: Option<String>,
    #[serde(default)]
    details: Vec<A2aErrorDetail>,
}

#[derive(Debug, Deserialize)]
struct A2aErrorDetail {
    #[serde(default)]
    reason: Option<String>,
}

/// Rewrite the first line of an API error when it is an A2A error document.
///
/// `Task cannot be canceled - current state: 3 (TASK_NOT_CANCELABLE)` replaces
/// the escaped JSON; the envelope's `error_code` and the HTTP transcript that
/// follow are kept. Errors of any other shape pass through untouched.
pub fn describe_a2a_error(err: Error) -> Error {
    let Error::Api { message, code } = err else {
        return err;
    };
    let Some((first, rest)) = split_first_line(&message) else {
        return Error::Api { message, code };
    };
    let Some(summary) = summarize(first) else {
        return Error::Api { message, code };
    };
    let message = match rest {
        Some(rest) => format!("{summary}\n{rest}"),
        None => summary,
    };
    Error::Api { message, code }
}

fn split_first_line(message: &str) -> Option<(&str, Option<&str>)> {
    match message.split_once('\n') {
        Some((first, rest)) => Some((first, Some(rest))),
        None if !message.is_empty() => Some((message, None)),
        None => None,
    }
}

/// `<inner message> (<reason>)` if `line` is an A2A error document, possibly
/// followed by the ` [ERROR_CODE]` suffix the envelope decoder appends.
fn summarize(line: &str) -> Option<String> {
    let (json, suffix) = match line.rsplit_once(" [") {
        Some((json, tail)) if tail.ends_with(']') && json.ends_with('}') => {
            (json, Some(&line[json.len()..]))
        }
        _ => (line, None),
    };
    let document: A2aErrorDocument = serde_json::from_str(json).ok()?;

    let message = document
        .error
        .message
        .filter(|m| !m.trim().is_empty())
        .unwrap_or_else(|| "A2A request failed".to_string());
    let reason = document
        .error
        .details
        .iter()
        .find_map(|detail| detail.reason.as_deref().filter(|r| !r.trim().is_empty()));

    let mut summary = message;
    if let Some(reason) = reason {
        summary.push_str(&format!(" ({reason})"));
    }
    if let Some(suffix) = suffix {
        summary.push_str(suffix);
    }
    Some(summary)
}

#[cfg(test)]
mod tests {
    use super::*;

    const DOC: &str = r#"{"error":{"code":404,"status":"NOT_FOUND","message":"Task not found","details":[{"@type":"type.googleapis.com/google.rpc.ErrorInfo","reason":"TASK_NOT_FOUND","domain":"a2a-protocol.org","metadata":{}}]}}"#;

    fn api(message: String) -> Error {
        Error::Api {
            message,
            code: Some("NOT_FOUND".into()),
        }
    }

    #[test]
    fn replaces_the_escaped_document_with_message_and_reason() {
        let err = describe_a2a_error(api(format!("{DOC} [NOT_FOUND]\nHTTP 404\n{{...}}")));
        assert_eq!(
            err.to_string(),
            "Task not found (TASK_NOT_FOUND) [NOT_FOUND]\nHTTP 404\n{...}"
        );
        assert!(matches!(err, Error::Api { code: Some(c), .. } if c == "NOT_FOUND"));
    }

    #[test]
    fn works_without_the_code_suffix_or_a_transcript() {
        assert_eq!(
            describe_a2a_error(api(DOC.to_string())).to_string(),
            "Task not found (TASK_NOT_FOUND)"
        );
    }

    #[test]
    fn a_document_without_details_keeps_just_the_message() {
        let doc = r#"{"error":{"code":500,"message":"boom"}}"#;
        assert_eq!(describe_a2a_error(api(doc.to_string())).to_string(), "boom");
    }

    #[test]
    fn ordinary_errors_pass_through_unchanged() {
        let plain = "You don't have the \"Chat (A2A)\" permission [ACCESS_DENIED]\nHTTP 403";
        assert_eq!(
            describe_a2a_error(api(plain.to_string())).to_string(),
            plain
        );

        let not_api = Error::NotLoggedIn;
        assert!(matches!(describe_a2a_error(not_api), Error::NotLoggedIn));
    }

    #[test]
    fn json_that_is_not_an_a2a_document_passes_through() {
        let other = r#"{"foo":"bar"} [X]"#;
        assert_eq!(
            describe_a2a_error(api(other.to_string())).to_string(),
            other
        );
    }
}
