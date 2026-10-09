//! Wire-level `conversation` tests against a loopback stub of the API.
//!
//! Two things about these endpoints are easy to get wrong and cheap to pin
//! here: conversations live under `workspaces/{id}/memories/conversations`
//! while their messages live at `conversations/{id}/messages` with no
//! workspace segment at all, and an omitted optional field must be absent from
//! the body rather than sent as `null` or `[]`.

use serde_json::{Value, json};

use crate::common::stub::{exchange, exchange_sequence, request_body, request_line};
use crate::common::{assert_failure, assert_success};

const CONVERSATION: &str = r#"{"success":true,"data":{"id":"conv-1","kind":"DIRECT"}}"#;
const EMPTY_PAGE: &str = r#"{"success":true,"data":{"items":[]}}"#;
const MESSAGE: &str = r#"{"success":true,"data":{"id":"conv-entry-1"}}"#;
const EMPTY_OBJECT: &str = r#"{"success":true,"data":{}}"#;

fn body_of(request: &str) -> Value {
    serde_json::from_str(request_body(request)).expect("request body should be JSON")
}

#[test]
fn create_posts_to_the_workspace_memories_tree() {
    let (request, output) = exchange(
        CONVERSATION,
        &[
            "conversation",
            "create",
            "--workspace",
            "ws-1",
            "--custom-id",
            "session-42",
            "--project",
            "proj-1",
            "--actors",
            "actor-1",
        ],
    );
    assert_success(&output, &["conversation", "create"]);

    assert_eq!(
        request_line(&request),
        "POST /api/v3/workspaces/ws-1/memories/conversations HTTP/1.1"
    );
}

#[test]
fn create_sends_only_what_was_asked_for() {
    // `name` and `metadata` were not given: they must be absent, not null and
    // not empty, so the server applies its own defaults. `actor_ids` is not
    // one of them — the API requires it, so it is always on the wire.
    let (request, _) = exchange(
        CONVERSATION,
        &[
            "conversation",
            "create",
            "--workspace",
            "ws-1",
            "--custom-id",
            "session-42",
            "--project",
            "proj-1",
            "--actors",
            "actor-1",
        ],
    );
    assert_eq!(
        body_of(&request),
        json!({
            "custom_id": "session-42",
            "kind": "DIRECT",
            "rw_project_ids": ["proj-1"],
            "actor_ids": ["actor-1"]
        })
    );
}

#[test]
fn create_maps_every_flag_onto_its_documented_field() {
    let (request, output) = exchange(
        CONVERSATION,
        &[
            "conversation",
            "create",
            "--workspace",
            "ws-1",
            "--custom-id",
            "session-42",
            "--project",
            "proj-1",
            "--name",
            "Q3 Planning",
            "--kind",
            "GROUP",
            "--actors",
            "actor-1,actor-2",
            "--metadata",
            "team=core",
            "--metadata",
            "source=cli",
        ],
    );
    assert_success(&output, &["conversation", "create"]);

    assert_eq!(
        body_of(&request),
        json!({
            "custom_id": "session-42",
            "kind": "GROUP",
            "rw_project_ids": ["proj-1"],
            "name": "Q3 Planning",
            "actor_ids": ["actor-1", "actor-2"],
            "metadata": {"team": "core", "source": "cli"}
        }),
        "the API takes the project as a one-element `rw_project_ids` list"
    );
}

#[test]
fn list_passes_paging_through_as_query_parameters() {
    let (request, output) = exchange(
        EMPTY_PAGE,
        &[
            "conversation",
            "list",
            "--workspace",
            "ws-1",
            "--page-size",
            "5",
            "--continuation-token",
            "tok-1",
        ],
    );
    assert_success(&output, &["conversation", "list"]);

    assert_eq!(
        request_line(&request),
        "GET /api/v3/workspaces/ws-1/memories/conversations?page_size=5&continuation_token=tok-1 HTTP/1.1"
    );
}

#[test]
fn list_without_paging_sends_no_query_string() {
    let (request, _) = exchange(EMPTY_PAGE, &["conversation", "list", "--workspace", "ws-1"]);
    assert_eq!(
        request_line(&request),
        "GET /api/v3/workspaces/ws-1/memories/conversations HTTP/1.1"
    );
}

#[test]
fn get_addresses_the_conversation_by_id_by_default() {
    let (request, output) = exchange(
        CONVERSATION,
        &["conversation", "get", "--workspace", "ws-1", "conv-1"],
    );
    assert_success(&output, &["conversation", "get"]);

    assert_eq!(
        request_line(&request),
        "GET /api/v3/workspaces/ws-1/memories/conversations/conv-1 HTTP/1.1",
        "no by_custom_id unless it was asked for"
    );
}

#[test]
fn get_by_custom_id_switches_the_lookup() {
    let (request, _) = exchange(
        CONVERSATION,
        &[
            "conversation",
            "get",
            "--workspace",
            "ws-1",
            "session-42",
            "--by-custom-id",
        ],
    );
    assert_eq!(
        request_line(&request),
        "GET /api/v3/workspaces/ws-1/memories/conversations/session-42?by_custom_id=true HTTP/1.1"
    );
}

#[test]
fn cook_status_reads_the_documented_subresource() {
    let (request, output) = exchange(
        r#"{"success":true,"data":{"conversation_id":"conv-1","cook_finished":true}}"#,
        &[
            "conversation",
            "cook-status",
            "--workspace",
            "ws-1",
            "conv-1",
        ],
    );
    let stdout = assert_success(&output, &["conversation", "cook-status"]);

    assert_eq!(
        request_line(&request),
        "GET /api/v3/workspaces/ws-1/memories/conversations/conv-1/cook-status HTTP/1.1"
    );
    assert!(
        stdout.contains("\"cook_finished\": true"),
        "the flag must reach the caller: {stdout}"
    );
}

#[test]
fn cook_status_by_custom_id_switches_the_lookup() {
    let (request, _) = exchange(
        r#"{"success":true,"data":{"cook_finished":false}}"#,
        &[
            "conversation",
            "cook-status",
            "--workspace",
            "ws-1",
            "session-42",
            "--by-custom-id",
        ],
    );
    assert_eq!(
        request_line(&request),
        "GET /api/v3/workspaces/ws-1/memories/conversations/session-42/cook-status?by_custom_id=true HTTP/1.1"
    );
}

#[test]
fn delete_uses_the_delete_method_and_confirms_what_it_removed() {
    let (request, output) = exchange(
        EMPTY_OBJECT,
        &["conversation", "delete", "--workspace", "ws-1", "conv-1"],
    );
    let stdout = assert_success(&output, &["conversation", "delete"]);

    assert_eq!(
        request_line(&request),
        "DELETE /api/v3/workspaces/ws-1/memories/conversations/conv-1 HTTP/1.1"
    );
    assert!(
        stdout.contains("deleted conversation conv-1"),
        "delete names what it removed: {stdout}"
    );
}

#[test]
fn message_append_takes_no_workspace_segment() {
    // The messages endpoints hang off the conversation alone. Building them
    // under the workspace tree would 404, so pin the exact path.
    let (request, output) = exchange(
        MESSAGE,
        &[
            "conversation",
            "message",
            "append",
            "conv-1",
            "--actor",
            "actor-1",
            "--custom-id",
            "msg-42",
            "--text",
            "hello",
            // Naming the parent keeps this to a single request: without it the
            // command first looks up the conversation's head.
            "--parent",
            "conv-entry-7",
        ],
    );
    assert_success(&output, &["conversation", "message", "append"]);

    assert_eq!(
        request_line(&request),
        "POST /api/v3/conversations/conv-1/messages HTTP/1.1"
    );
}

#[test]
fn message_append_turns_each_text_flag_into_one_text_block() {
    let (request, _) = exchange(
        MESSAGE,
        &[
            "conversation",
            "message",
            "append",
            "conv-1",
            "--actor",
            "actor-1",
            "--custom-id",
            "msg-42",
            "--text",
            "first",
            "--text",
            "second",
            "--parent",
            "conv-entry-7",
        ],
    );
    assert_eq!(
        body_of(&request),
        json!({
            "actor_id": "actor-1",
            "custom_id": "msg-42",
            "content": [
                {"block_type": "TEXT", "text": "first"},
                {"block_type": "TEXT", "text": "second"}
            ],
            "parent_message_id": "conv-entry-7"
        }),
        "unset optionals stay absent, but the parent is always sent"
    );
}

#[test]
fn message_append_forwards_json_blocks_verbatim() {
    let (request, output) = exchange(
        MESSAGE,
        &[
            "conversation",
            "message",
            "append",
            "conv-1",
            "--actor",
            "actor-1",
            "--custom-id",
            "msg-43",
            "--content-json",
            r#"[{"block_type":"TOOL_USE","tool_call_id":"c1","tool_name":"search","arguments":{"q":"revenue"}}]"#,
            "--parent",
            "conv-entry-8",
            "--timestamp",
            "2026-08-13T00:00:00Z",
            "--metadata",
            "source=cli",
        ],
    );
    assert_success(&output, &["conversation", "message", "append"]);

    assert_eq!(
        body_of(&request),
        json!({
            "actor_id": "actor-1",
            "custom_id": "msg-43",
            "content": [{
                "block_type": "TOOL_USE",
                "tool_call_id": "c1",
                "tool_name": "search",
                "arguments": {"q": "revenue"}
            }],
            "parent_message_id": "conv-entry-8",
            "timestamp": "2026-08-13T00:00:00Z",
            "metadata": {"source": "cli"}
        })
    );
}

#[test]
fn append_wait_polls_cook_status_until_it_reports_finished() {
    let (requests, output) = exchange_sequence(
        &[
            MESSAGE,
            r#"{"success":true,"data":{"cook_finished":false}}"#,
            r#"{"success":true,"data":{"cook_finished":true}}"#,
        ],
        &[
            "conversation",
            "message",
            "append",
            "conv-1",
            "--actor",
            "actor-1",
            "--custom-id",
            "msg-42",
            "--text",
            "hello",
            "--parent",
            "conv-entry-7",
            "--wait",
            "--workspace",
            "ws-1",
        ],
    );
    let stdout = assert_success(&output, &["conversation", "message", "append", "--wait"]);

    assert_eq!(
        request_line(&requests[0]),
        "POST /api/v3/conversations/conv-1/messages HTTP/1.1"
    );
    for poll in &requests[1..] {
        assert_eq!(
            request_line(poll),
            "GET /api/v3/workspaces/ws-1/memories/conversations/conv-1/cook-status HTTP/1.1",
            "--wait polls the workspace-scoped cook-status endpoint"
        );
    }
    assert!(
        stdout.contains("conv-entry-1"),
        "the appended message still prints; --wait only decides when to return: {stdout}"
    );
}

#[test]
fn append_wait_fails_when_the_timeout_elapses_first() {
    // `--timeout 0` gives up after the first poll, so this pins the giving-up
    // path without spending the backoff schedule.
    let (requests, output) = exchange_sequence(
        &[
            MESSAGE,
            r#"{"success":true,"data":{"cook_finished":false}}"#,
        ],
        &[
            "conversation",
            "message",
            "append",
            "conv-1",
            "--actor",
            "actor-1",
            "--custom-id",
            "msg-42",
            "--text",
            "hello",
            "--parent",
            "conv-entry-7",
            "--wait",
            "--workspace",
            "ws-1",
            "--timeout",
            "0",
        ],
    );
    let err = assert_failure(&output, &["conversation", "message", "append", "--wait"]);

    assert_eq!(requests.len(), 2, "one append, one poll, then give up");
    assert!(
        err.contains("still building its memory"),
        "the failure names what timed out: {err}"
    );
    assert!(
        err.contains("processing continues on the server"),
        "and that giving up did not cancel anything: {err}"
    );
    assert!(
        err.contains("conv-entry-1"),
        "the message id is still reported — it was appended: {err}"
    );
}

#[test]
fn append_without_wait_sends_exactly_one_request() {
    let (requests, output) = exchange_sequence(
        &[MESSAGE],
        &[
            "conversation",
            "message",
            "append",
            "conv-1",
            "--actor",
            "actor-1",
            "--custom-id",
            "msg-42",
            "--text",
            "hello",
            "--parent",
            "conv-entry-7",
        ],
    );
    assert_success(&output, &["conversation", "message", "append"]);
    assert_eq!(
        requests.len(),
        1,
        "no polling unless --wait was asked for: {requests:?}"
    );
}

#[test]
fn message_list_reads_the_conversation_scoped_path() {
    let (request, output) = exchange(
        EMPTY_PAGE,
        &[
            "conversation",
            "message",
            "list",
            "conv-1",
            "--page-size",
            "3",
        ],
    );
    assert_success(&output, &["conversation", "message", "list"]);

    assert_eq!(
        request_line(&request),
        "GET /api/v3/conversations/conv-1/messages?page_size=3 HTTP/1.1"
    );
}

#[test]
fn ids_are_percent_encoded_into_their_own_segment() {
    let (request, _) = exchange(
        EMPTY_PAGE,
        &["conversation", "message", "list", "conv/../x"],
    );
    assert_eq!(
        request_line(&request),
        "GET /api/v3/conversations/conv%2F..%2Fx/messages HTTP/1.1",
        "a traversal attempt must not escape its segment"
    );
}

#[test]
fn append_without_a_parent_looks_up_the_head_first() {
    // The API requires `parent_message_id` and rejects an append that omits it
    // on a conversation that already has messages. Rather than pass that on to
    // the caller, the command reads the conversation and uses the head it
    // reports.
    let (requests, output) = exchange_sequence(
        &[
            r#"{"success":true,"data":{"id":"conv-1","current_message_id":"conv-entry-9"}}"#,
            MESSAGE,
        ],
        &[
            "conversation",
            "message",
            "append",
            "conv-1",
            "--actor",
            "actor-1",
            "--custom-id",
            "msg-42",
            "--text",
            "hello",
            "--workspace",
            "ws-1",
        ],
    );
    assert_success(&output, &["conversation", "message", "append"]);

    assert_eq!(
        request_line(&requests[0]),
        "GET /api/v3/workspaces/ws-1/memories/conversations/conv-1 HTTP/1.1",
        "the head is read from the conversation"
    );
    assert_eq!(
        request_line(&requests[1]),
        "POST /api/v3/conversations/conv-1/messages HTTP/1.1"
    );
    assert_eq!(
        body_of(&requests[1])["parent_message_id"],
        "conv-entry-9",
        "the head reported by the conversation is sent as the parent"
    );
}

#[test]
fn the_first_message_in_a_conversation_sends_a_null_parent() {
    // An empty conversation reports no head. The field is still required, so
    // it goes out as null rather than being dropped.
    let (requests, output) = exchange_sequence(
        &[r#"{"success":true,"data":{"id":"conv-1"}}"#, MESSAGE],
        &[
            "conversation",
            "message",
            "append",
            "conv-1",
            "--actor",
            "actor-1",
            "--custom-id",
            "msg-1",
            "--text",
            "first",
            "--workspace",
            "ws-1",
        ],
    );
    assert_success(&output, &["conversation", "message", "append"]);

    let body = body_of(&requests[1]);
    assert!(
        body.as_object()
            .expect("object body")
            .contains_key("parent_message_id"),
        "the field must be present even when null: {body}"
    );
    assert_eq!(body["parent_message_id"], serde_json::Value::Null);
}

const FACT_ACTIONS: &str = r#"{"success":true,"data":{"items":[{"event":"ADD","history_id":"facthist-1","fact_id":"fact-1","new_fact":"Lives in Lisbon.","changed_at":"2026-10-09T07:44:21Z"}],"total":1}}"#;

#[test]
fn fact_actions_name_the_owner_in_the_query() {
    for (owner_flag, owner_type) in [("--actor", "actor"), ("--project", "project")] {
        let args = [
            "conversation",
            "fact-actions",
            "--workspace",
            "ws-1",
            "conv-1",
            owner_flag,
            "owner-1",
        ];
        let (request, output) = exchange(FACT_ACTIONS, &args);
        let stdout = assert_success(&output, &args);

        assert_eq!(
            request_line(&request),
            format!(
                "GET /api/v3/workspaces/ws-1/memories/conversations/conv-1/fact-actions?owner_type={owner_type}&owner_id=owner-1 HTTP/1.1"
            )
        );
        let printed: Value = serde_json::from_str(&stdout).expect("page JSON");
        assert_eq!(printed["items"][0]["event"], "ADD");
    }
}

#[test]
fn fact_actions_forward_every_filter() {
    let args = [
        "conversation",
        "fact-actions",
        "--workspace",
        "ws-1",
        "session-42",
        "--actor",
        "actor-1",
        "--message",
        "conv-entry-1",
        "--by-custom-id",
        "--page-size",
        "5",
        "--continuation-token",
        "tok",
    ];
    let (request, output) = exchange(FACT_ACTIONS, &args);
    assert_success(&output, &args);
    assert_eq!(
        request_line(&request),
        "GET /api/v3/workspaces/ws-1/memories/conversations/session-42/fact-actions?owner_type=actor&owner_id=actor-1&message_id=conv-entry-1&by_custom_id=true&page_size=5&continuation_token=tok HTTP/1.1"
    );
}

#[test]
fn consumed_messages_send_the_message_id_and_print_the_batch() {
    let response = r#"{"success":true,"data":[{"message_id":"conv-entry-1","custom_id":"m1","sequence_no":1}]}"#;
    let args = [
        "conversation",
        "consumed",
        "--workspace",
        "ws-1",
        "conv-1",
        "--message",
        "conv-entry-1",
    ];
    let (request, output) = exchange(response, &args);
    let stdout = assert_success(&output, &args);

    assert_eq!(
        request_line(&request),
        "GET /api/v3/workspaces/ws-1/memories/conversations/conv-1/consumed-messages?message_id=conv-entry-1 HTTP/1.1"
    );
    let printed: Value = serde_json::from_str(&stdout).expect("array JSON");
    assert_eq!(printed[0]["custom_id"], "m1");
}

#[test]
fn consumed_messages_can_address_the_conversation_by_custom_id() {
    let args = [
        "conversation",
        "consumed-messages",
        "--workspace",
        "ws-1",
        "session-42",
        "--message",
        "conv-entry-1",
        "--by-custom-id",
    ];
    let (request, output) = exchange(r#"{"success":true,"data":[]}"#, &args);
    assert_success(&output, &args);
    assert!(
        request_line(&request).ends_with(
            "/conversations/session-42/consumed-messages?message_id=conv-entry-1&by_custom_id=true HTTP/1.1"
        ),
        "{request}"
    );
}

#[test]
fn message_get_posts_the_ids_to_batch_get_without_a_workspace() {
    let response = r#"{"success":true,"data":[{"id":"conv-entry-2","sequence_no":2,"agent_id":"agent-1","content":[{"block_type":"TEXT","text":"hi"}]},{"id":"conv-entry-1","sequence_no":1,"content":[]}]}"#;
    let args = [
        "conversation",
        "message",
        "get",
        "conv-1",
        "conv-entry-2",
        "conv-entry-1",
    ];
    let (request, output) = exchange(response, &args);
    let stdout = assert_success(&output, &args);

    assert_eq!(
        request_line(&request),
        "POST /api/v3/conversations/conv-1/messages/batch-get HTTP/1.1"
    );
    assert_eq!(
        body_of(&request),
        json!({"entry_ids": ["conv-entry-2", "conv-entry-1"]})
    );
    let printed: Value = serde_json::from_str(&stdout).expect("array JSON");
    assert_eq!(printed[0]["agent_id"], "agent-1", "agent_id is surfaced");
    assert_eq!(printed[1]["id"], "conv-entry-1");
}
