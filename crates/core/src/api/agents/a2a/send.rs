//! Send a message to an agent (`POST .../a2a/message:send` and
//! `message:stream`, A2A v1.0).

use std::io::BufReader;

use serde_json::Value;

use crate::client::Client;
use crate::error::Result;
use crate::sse::EventStream;

use super::types::SendMessageRequest;
use super::{describe_a2a_error, message_send_path, message_stream_path};

/// Deliver a message and return the resulting task.
///
/// The server blocks until the task finishes unless
/// `configuration.returnImmediately` is set. The response is
/// `{"task": {...}}` — or, for a reply that produced no task, `{"message":
/// {...}}` — exactly as the protocol defines it.
pub fn send_message(
    client: &Client,
    workspace_id: &str,
    agent_id: &str,
    request: &SendMessageRequest,
) -> Result<Value> {
    client
        .post_json_with_headers(&message_send_path(workspace_id, agent_id), request, &[])
        .map_err(describe_a2a_error)
}

/// Deliver a message and follow the task as a stream of events.
///
/// Each event is one A2A `StreamResponse`: the first carries `task`, the
/// following ones `statusUpdate` (whose `status.message.parts` hold the
/// agent's text as it is produced) or `artifactUpdate`, and the last has a
/// terminal `status.state`.
pub fn stream_message(
    client: &Client,
    workspace_id: &str,
    agent_id: &str,
    request: &SendMessageRequest,
) -> Result<EventStream<BufReader<reqwest::blocking::Response>>> {
    client
        .post_event_stream(&message_stream_path(workspace_id, agent_id), request, &[])
        .map_err(describe_a2a_error)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::api::agents::a2a::{Message, ROLE_USER, text_part};
    use crate::test_support::{json_ok, one_shot_server};

    fn request() -> SendMessageRequest {
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
    fn send_posts_to_the_unversioned_v1_path() {
        let task = r#"{"task":{"id":"run-1","status":{"state":"TASK_STATE_COMPLETED"}}}"#;
        let (base_url, server) = one_shot_server(json_ok(task));
        let client = Client::new(base_url, "sk-test").unwrap();

        let value = send_message(&client, "ws-1", "agt-1", &request()).expect("send");
        assert_eq!(value["task"]["id"], "run-1");

        let captured = server.join().unwrap();
        assert!(
            captured
                .head
                .starts_with("POST /api/v3/workspaces/ws-1/agents/agt-1/a2a/message:send "),
            "{}",
            captured.head
        );
        let body: Value = serde_json::from_slice(&captured.body).unwrap();
        assert_eq!(body["message"]["parts"][0]["text"], "hi");
    }

    #[test]
    fn stream_reads_events_until_the_body_ends() {
        let body = "data:{\"task\":{\"id\":\"run-1\"}}\n\ndata:{\"statusUpdate\":{\"status\":{\"state\":\"TASK_STATE_COMPLETED\"}}}\n\n";
        let response = format!(
            "HTTP/1.1 200 OK\r\nContent-Type: text/event-stream\r\nContent-Length: {}\r\n\r\n{body}",
            body.len()
        );
        let (base_url, server) = one_shot_server(response);
        let client = Client::new(base_url, "sk-test").unwrap();

        let events: Vec<Value> = stream_message(&client, "ws-1", "agt-1", &request())
            .expect("open stream")
            .collect::<Result<_>>()
            .expect("read events");
        assert_eq!(events.len(), 2);
        assert_eq!(events[0]["task"]["id"], "run-1");
        assert_eq!(
            events[1]["statusUpdate"]["status"]["state"],
            "TASK_STATE_COMPLETED"
        );

        let captured = server.join().unwrap();
        assert!(
            captured
                .head
                .starts_with("POST /api/v3/workspaces/ws-1/agents/agt-1/a2a/message:stream "),
            "{}",
            captured.head
        );
        assert!(captured.has_header("accept"), "{}", captured.head);
    }

    #[test]
    fn stream_answered_with_a_json_envelope_is_the_envelope_error() {
        // Observed in production: a permission failure on `message:stream`
        // comes back as a plain envelope, not as an event stream.
        let body = r#"{"success":false,"message":"You don't have the \"Chat (A2A)\" permission","error_code":"ACCESS_DENIED"}"#;
        let (base_url, _server) = one_shot_server(format!(
            "HTTP/1.1 403 Forbidden\r\nContent-Type: application/json\r\nContent-Length: {}\r\n\r\n{body}",
            body.len()
        ));
        let client = Client::new(base_url, "sk-test").unwrap();

        let err = stream_message(&client, "ws-1", "agt-1", &request()).expect_err("denied");
        assert!(err.to_string().contains("Chat (A2A)"), "{err}");
        assert!(
            matches!(&err, crate::Error::Api { code: Some(code), .. } if code == "ACCESS_DENIED"),
            "{err:?}"
        );
    }

    #[test]
    fn a_2xx_that_is_not_an_event_stream_is_rejected() {
        let (base_url, _server) = one_shot_server(json_ok(r#"{"success":true,"data":{}}"#));
        let client = Client::new(base_url, "sk-test").unwrap();

        let err = stream_message(&client, "ws-1", "agt-1", &request()).expect_err("not a stream");
        assert!(err.to_string().contains("text/event-stream"), "{err}");
    }
}
