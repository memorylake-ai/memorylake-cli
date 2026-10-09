//! Wire-level `fact` tests against a loopback stub of the MemoryLake API.
//!
//! These pin the method, path, query string, and body each subcommand sends
//! for every scope shape — the project scope alone carries `/memories/` before
//! `facts` — and that the payload is printed back as JSON.

use serde_json::{Value, json};

use crate::common::assert_success;
use crate::common::stub::{
    exchange, exchange_with_remembered_workspace, request_body, request_line,
};

const FACT: &str = r#"{"success":true,"data":{"id":"fact-1","fact":"t","metadata":{"k":"v"},"expired":false,"created_at":"2026-10-09T07:40:45Z","updated_at":"2026-10-09T07:41:00Z"}}"#;
const VOID: &str = r#"{"success":true}"#;
const EMPTY_PAGE: &str = r#"{"success":true,"data":{"items":[],"total":0}}"#;
const SETTINGS: &str = r#"{"success":true,"data":{"fact_instruction":"Records deadlines."}}"#;
const CONFLICT: &str = r#"{"success":true,"data":{"id":"cfl-1","category":"self","resolved":false,"fact_ids":["fact-1"],"fact_snapshots":[],"file_chunks":[]}}"#;
const RESOLVED: &str = r#"{"success":true,"data":{"id":"cfl-1","resolved":true,"strategy":"edit_fact","resolve_id":"cflr-1","forgotten_fact_ids":[],"updated_fact_ids":["fact-1"]}}"#;

fn body_of(request: &str) -> Value {
    serde_json::from_str(request_body(request)).expect("request body should be JSON")
}

fn stdout_json(output: &std::process::Output, args: &[&str]) -> Value {
    let stdout = assert_success(output, args);
    serde_json::from_str(&stdout).expect("stdout should be JSON")
}

#[test]
fn add_to_an_agent_posts_to_the_agent_facts_collection() {
    let args = [
        "fact",
        "add",
        "--workspace",
        "ws-1",
        "--agent",
        "agent-1",
        "one",
        "two",
    ];
    let (request, output) = exchange(
        r#"{"success":true,"data":{"facts":[{"id":"fact-1","fact":"one"},{"id":"fact-2","fact":"two"}]}}"#,
        &args,
    );
    assert_eq!(
        request_line(&request),
        "POST /api/v3/workspaces/ws-1/agents/agent-1/facts HTTP/1.1"
    );
    assert_eq!(body_of(&request), json!({"facts": ["one", "two"]}));
    // Fields the server did not send are not printed as null.
    let printed = stdout_json(&output, &args);
    assert_eq!(
        printed["facts"][0],
        json!({"id": "fact-1", "fact": "one", "expired": false})
    );
}

#[test]
fn delete_from_an_agent_forgets_each_id_under_the_agent() {
    let args = [
        "fact",
        "delete",
        "--workspace",
        "ws-1",
        "--agent",
        "agent-1",
        "fact-1",
    ];
    let (request, output) = exchange(VOID, &args);
    assert_eq!(
        request_line(&request),
        "POST /api/v3/workspaces/ws-1/agents/agent-1/facts/fact-1/forget HTTP/1.1"
    );
    assert_eq!(
        stdout_json(&output, &args),
        json!({"forgotten": ["fact-1"], "not_found": []})
    );
}

#[test]
fn get_reads_one_fact_in_each_scope_shape() {
    for (scope, id, path) in [
        ("--actor", "actor-1", "actors/actor-1/facts/fact-1"),
        (
            "--project",
            "proj-1",
            "projects/proj-1/memories/facts/fact-1",
        ),
        ("--agent", "agent-1", "agents/agent-1/facts/fact-1"),
    ] {
        let args = ["fact", "get", "--workspace", "ws-1", scope, id, "fact-1"];
        let (request, output) = exchange(FACT, &args);
        assert_eq!(
            request_line(&request),
            format!("GET /api/v3/workspaces/ws-1/{path} HTTP/1.1")
        );
        assert_eq!(stdout_json(&output, &args)["metadata"], json!({"k": "v"}));
    }
}

#[test]
fn get_uses_the_remembered_workspace() {
    let args = ["fact", "get", "--project", "proj-1", "fact-1"];
    let (request, output) = exchange_with_remembered_workspace(FACT, "ws-remembered", &args);
    assert_success(&output, &args);
    assert_eq!(
        request_line(&request),
        "GET /api/v3/workspaces/ws-remembered/projects/proj-1/memories/facts/fact-1 HTTP/1.1"
    );
}

#[test]
fn update_patches_text_and_metadata() {
    let args = [
        "fact",
        "update",
        "--workspace",
        "ws-1",
        "--project",
        "proj-1",
        "fact-1",
        "--text",
        "new text",
        "--metadata",
        r#"{"source":"cli","n":2}"#,
    ];
    let (request, output) = exchange(FACT, &args);
    assert_success(&output, &args);
    assert_eq!(
        request_line(&request),
        "PATCH /api/v3/workspaces/ws-1/projects/proj-1/memories/facts/fact-1 HTTP/1.1"
    );
    assert_eq!(
        body_of(&request),
        json!({"fact": "new text", "metadata": {"source": "cli", "n": 2}})
    );
}

#[test]
fn update_sends_only_the_fields_given() {
    let args = [
        "fact",
        "update",
        "--workspace",
        "ws-1",
        "--actor",
        "actor-1",
        "fact-1",
        "--metadata",
        "{}",
    ];
    let (request, _) = exchange(FACT, &args);
    assert_eq!(body_of(&request), json!({"metadata": {}}));

    let args = [
        "fact",
        "update",
        "--workspace",
        "ws-1",
        "--actor",
        "actor-1",
        "fact-1",
        "--text",
        "only text",
    ];
    let (request, _) = exchange(FACT, &args);
    assert_eq!(body_of(&request), json!({"fact": "only text"}));
}

#[test]
fn trace_reads_the_history_and_prints_it_unchanged() {
    let response = r#"{"success":true,"data":{"fact":{"id":"fact-1","fact":"t","expired":true,"expiration_date":"2026-10-09T07:41:35Z"},"trace":[{"event":"FORGET","source_kind":"MANUAL","old_fact":"t","source_entry_ids":[]}]}}"#;
    let args = [
        "fact",
        "trace",
        "--workspace",
        "ws-1",
        "--agent",
        "agent-1",
        "fact-1",
    ];
    let (request, output) = exchange(response, &args);
    assert_eq!(
        request_line(&request),
        "GET /api/v3/workspaces/ws-1/agents/agent-1/facts/fact-1/trace HTTP/1.1"
    );
    let printed = stdout_json(&output, &args);
    let sent: Value = serde_json::from_str(response).expect("response JSON");
    assert_eq!(printed, sent["data"]);
}

#[test]
fn list_sends_every_owner_kind_and_the_query() {
    let args = [
        "fact",
        "list",
        "--workspace",
        "ws-1",
        "--actors",
        "actor-1,actor-2",
        "--projects",
        "proj-1",
        "--agents",
        "agent-1",
        "--query",
        "editor",
        "--page-size",
        "200",
        "--continuation-token",
        "tok",
    ];
    let (request, output) = exchange(EMPTY_PAGE, &args);
    assert_success(&output, &args);
    assert_eq!(
        request_line(&request),
        "GET /api/v3/workspaces/ws-1/memories/facts?actor_ids=actor-1&actor_ids=actor-2\
         &project_ids=proj-1&agent_ids=agent-1&fact_fuzzy=editor&page_size=200\
         &continuation_token=tok HTTP/1.1"
    );
}

#[test]
fn conflict_list_sends_every_filter_under_its_wire_name() {
    let args = [
        "fact",
        "conflict",
        "list",
        "--workspace",
        "ws-1",
        "--actor",
        "actor-1",
        "--resolved",
        "false",
        "--category",
        "self",
        "--conflict-type",
        "logical",
        "--stale",
        "true",
        "--page-size",
        "100",
    ];
    let (request, output) = exchange(EMPTY_PAGE, &args);
    assert_success(&output, &args);
    assert_eq!(
        request_line(&request),
        "GET /api/v3/workspaces/ws-1/actors/actor-1/memories/conflicts?resolved=false\
         &category=self&conflict_type=logical&stale=true&page_size=100 HTTP/1.1"
    );
}

#[test]
fn conflict_list_without_filters_sends_no_query() {
    let args = [
        "fact",
        "conflict",
        "list",
        "--workspace",
        "ws-1",
        "--project",
        "proj-1",
    ];
    let (request, _) = exchange(EMPTY_PAGE, &args);
    assert_eq!(
        request_line(&request),
        "GET /api/v3/workspaces/ws-1/projects/proj-1/memories/conflicts HTTP/1.1"
    );
}

#[test]
fn conflict_get_reads_one_conflict() {
    let args = [
        "fact",
        "conflict",
        "get",
        "--workspace",
        "ws-1",
        "--agent",
        "agent-1",
        "cfl-1",
    ];
    let (request, output) = exchange(CONFLICT, &args);
    assert_eq!(
        request_line(&request),
        "GET /api/v3/workspaces/ws-1/agents/agent-1/memories/conflicts/cfl-1 HTTP/1.1"
    );
    assert_eq!(stdout_json(&output, &args)["category"], "self");
}

#[test]
fn conflict_resolve_sends_only_the_strategy_inputs() {
    let base = [
        "fact",
        "conflict",
        "resolve",
        "--workspace",
        "ws-1",
        "--project",
        "proj-1",
        "cfl-1",
    ];
    let cases: &[(&[&str], Value)] = &[
        (&["--strategy", "dismiss"], json!({"strategy": "dismiss"})),
        (
            &["--strategy", "keep_fact", "--keep-fact-id", "fact-2"],
            json!({"strategy": "keep_fact", "keep_fact_id": "fact-2"}),
        ),
        (
            &["--strategy", "trust_document"],
            json!({"strategy": "trust_document"}),
        ),
        (
            &[
                "--strategy",
                "edit_fact",
                "--edit",
                "fact-1=a = b",
                "--edit",
                "fact-2=c",
            ],
            json!({
                "strategy": "edit_fact",
                "edits": [
                    {"fact_id": "fact-1", "new_fact_text": "a = b"},
                    {"fact_id": "fact-2", "new_fact_text": "c"}
                ]
            }),
        ),
    ];
    for (flags, expected) in cases {
        let mut args = base.to_vec();
        args.extend_from_slice(flags);
        let (request, output) = exchange(RESOLVED, &args);
        assert_success(&output, &args);
        assert_eq!(
            request_line(&request),
            "POST /api/v3/workspaces/ws-1/projects/proj-1/memories/conflicts/cfl-1/resolve HTTP/1.1"
        );
        assert_eq!(&body_of(&request), expected, "{flags:?}");
    }
}

#[test]
fn instruction_get_reads_the_scope_settings() {
    for (scope, id, path) in [
        ("--actor", "actor-1", "actors/actor-1"),
        ("--project", "proj-1", "projects/proj-1"),
        ("--agent", "agent-1", "agents/agent-1"),
    ] {
        let args = [
            "fact",
            "instruction",
            "get",
            "--workspace",
            "ws-1",
            scope,
            id,
        ];
        let (request, output) = exchange(SETTINGS, &args);
        assert_eq!(
            request_line(&request),
            format!("GET /api/v3/workspaces/ws-1/{path}/settings HTTP/1.1")
        );
        assert_eq!(
            stdout_json(&output, &args),
            json!({"fact_instruction": "Records deadlines."})
        );
    }
}

#[test]
fn instruction_set_patches_the_text_verbatim() {
    let args = [
        "fact",
        "instruction",
        "set",
        "--workspace",
        "ws-1",
        "--project",
        "proj-1",
        "--text",
        "Records deadlines.\n\n## Include\n- dates",
    ];
    let (request, output) = exchange(SETTINGS, &args);
    assert_success(&output, &args);
    assert_eq!(
        request_line(&request),
        "PATCH /api/v3/workspaces/ws-1/projects/proj-1/settings HTTP/1.1"
    );
    assert_eq!(
        body_of(&request),
        json!({"fact_instruction": "Records deadlines.\n\n## Include\n- dates"})
    );
}

#[test]
fn instruction_set_reads_a_markdown_file() {
    let dir = std::env::temp_dir().join(crate::common::unique_name("fact-instruction"));
    std::fs::create_dir_all(&dir).expect("create scratch dir");
    let path = dir.join("instruction.md");
    std::fs::write(&path, "# 记录\n\n截止日期\n").expect("write instruction");
    let path = path.to_string_lossy().into_owned();

    let args = [
        "fact",
        "instruction",
        "set",
        "--workspace",
        "ws-1",
        "--agent",
        "agent-1",
        "--file",
        path.as_str(),
    ];
    let (request, output) = exchange(SETTINGS, &args);
    let _ = std::fs::remove_dir_all(&dir);
    assert_success(&output, &args);
    assert_eq!(
        body_of(&request),
        json!({"fact_instruction": "# 记录\n\n截止日期\n"})
    );
}

#[test]
fn instruction_clear_sends_an_empty_string() {
    let args = [
        "fact",
        "instruction",
        "clear",
        "--workspace",
        "ws-1",
        "--actor",
        "actor-1",
    ];
    let (request, output) = exchange(r#"{"success":true,"data":{"fact_instruction":""}}"#, &args);
    assert_success(&output, &args);
    assert_eq!(
        request_line(&request),
        "PATCH /api/v3/workspaces/ws-1/actors/actor-1/settings HTTP/1.1"
    );
    assert_eq!(body_of(&request), json!({"fact_instruction": ""}));
}

#[test]
fn instruction_draft_sends_only_the_options_given() {
    let base = [
        "fact",
        "instruction",
        "draft",
        "--workspace",
        "ws-1",
        "--project",
        "proj-1",
    ];
    let draft = r#"{"success":true,"data":{"fact_instruction":"Drafted."}}"#;

    let (request, output) = exchange(draft, &base);
    assert_eq!(
        request_line(&request),
        "POST /api/v3/workspaces/ws-1/projects/proj-1/settings/fact-instruction/draft HTTP/1.1"
    );
    assert_eq!(body_of(&request), json!({}));
    assert_eq!(
        stdout_json(&output, &base),
        json!({"fact_instruction": "Drafted."})
    );

    let mut args = base.to_vec();
    args.extend_from_slice(&[
        "--guidance",
        "Only deadlines",
        "--language",
        "Simplified Chinese",
        "--use-existing-facts",
        "--use-documents",
    ]);
    let (request, _) = exchange(draft, &args);
    assert_eq!(
        body_of(&request),
        json!({
            "guidance": "Only deadlines",
            "language": "Simplified Chinese",
            "use_existing_facts": true,
            "use_documents": true
        })
    );
}

#[test]
fn api_errors_surface_with_their_code() {
    let args = [
        "fact",
        "get",
        "--workspace",
        "ws-1",
        "--actor",
        "actor-1",
        "fact-missing",
    ];
    let (_, output) = exchange(
        r#"{"success":false,"message":"Fact not found with id: fact-missing","error_code":"NOT_FOUND"}"#,
        &args,
    );
    let err = crate::common::assert_failure(&output, &args);
    assert!(err.contains("NOT_FOUND"), "{err}");
    assert!(err.contains("get fact `fact-missing`"), "{err}");
}
