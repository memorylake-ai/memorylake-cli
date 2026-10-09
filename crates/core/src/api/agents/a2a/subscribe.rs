//! Follow a task's events (`GET .../a2a/tasks/{task}:subscribe`, A2A v1.0).

use std::io::BufReader;

use crate::client::Client;
use crate::error::Result;
use crate::sse::EventStream;

use super::{describe_a2a_error, task_subscribe_path};

/// Attach to a task and stream its events.
///
/// The events are the same `StreamResponse` documents `message:stream`
/// produces — `statusUpdate` and `artifactUpdate` — except that no leading
/// `task` event is sent. Production replays the task's events from the
/// beginning and then follows it live, so subscribing to a finished task is
/// not an error: it replays the whole run and ends at its terminal status
/// (measured 2026-10-09, contrary to the A2A spec, which reserves an error
/// for that case). An unknown task id is answered with `500 INTERNAL_ERROR`
/// rather than `TASK_NOT_FOUND`.
pub fn subscribe_task(
    client: &Client,
    workspace_id: &str,
    agent_id: &str,
    task_id: &str,
) -> Result<EventStream<BufReader<reqwest::blocking::Response>>> {
    client
        .get_event_stream(&task_subscribe_path(workspace_id, agent_id, task_id), &[])
        .map_err(describe_a2a_error)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test_support::one_shot_server;
    use serde_json::Value;

    #[test]
    fn subscribe_gets_the_v1_verb_path_and_reads_events() {
        let body = "data:{\"statusUpdate\":{\"taskId\":\"run-1\",\"status\":{\"state\":\"TASK_STATE_WORKING\"}}}\n\n\
                    data:{\"statusUpdate\":{\"taskId\":\"run-1\",\"status\":{\"state\":\"TASK_STATE_COMPLETED\"}}}\n\n";
        let (base_url, server) = one_shot_server(format!(
            "HTTP/1.1 200 OK\r\nContent-Type: text/event-stream\r\nContent-Length: {}\r\n\r\n{body}",
            body.len()
        ));
        let client = Client::new(base_url, "sk-test").unwrap();

        let events: Vec<Value> = subscribe_task(&client, "ws-1", "agt-1", "run-1")
            .expect("open stream")
            .collect::<Result<_>>()
            .expect("read events");
        assert_eq!(events.len(), 2);
        assert_eq!(
            events[1]["statusUpdate"]["status"]["state"],
            "TASK_STATE_COMPLETED"
        );

        let captured = server.join().unwrap();
        assert!(
            captured
                .head
                .starts_with("GET /api/v3/workspaces/ws-1/agents/agt-1/a2a/tasks/run-1:subscribe "),
            "{}",
            captured.head
        );
    }
}
