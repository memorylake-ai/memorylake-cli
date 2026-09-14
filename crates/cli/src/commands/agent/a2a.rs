//! `memorylake agent card|send|task`: talking to a bound agent over A2A.
//!
//! `send` is the conversation itself; `task` is the bookkeeping around it. Both
//! address the agent inside a workspace, so they take `--workspace` with the
//! same default as `agent bind`.
//!
//! Output is split so a script can pipe it: the agent's words go to stdout,
//! the ids needed to continue (task, context) go to stderr. `--raw` turns
//! that off and prints the protocol response as JSON.

use std::io::Write;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{SystemTime, UNIX_EPOCH};

use anyhow::{Context, Result, bail};
use clap::{Args, Subcommand, ValueEnum};
use memorylake_core::Client;
use memorylake_core::api::agents::a2a::{
    FEEDBACK_COMMENT_MAX_CHARS, ListTasksParams, MemorylakeExtension, Message, ROLE_USER, Rating,
    SendConfiguration, SendMessageRequest, SendMetadata, TASK_STATE_INPUT_REQUIRED,
    TaskFeedbackRequest, cancel_task, get_agent_card, get_task, list_tasks, send_message,
    stream_message, submit_task_feedback, text_part,
};
use serde_json::{Map, Value};

use super::super::print_json;

/// Arguments of `agent send`.
#[derive(Debug, Args)]
pub struct SendArgs {
    /// Agent to talk to (must be bound to the workspace).
    pub agent_id: String,
    /// Message text. Repeat for several text parts.
    #[arg(long, value_name = "TEXT")]
    pub text: Vec<String>,
    /// Full A2A message parts as an inline JSON array, for non-text parts.
    #[arg(long, value_name = "JSON")]
    pub message_json: Option<String>,
    /// Full A2A message parts read from a JSON file.
    #[arg(long, value_name = "PATH")]
    pub message_file: Option<PathBuf>,
    /// Continue an existing conversation thread.
    #[arg(long, value_name = "CONTEXT_ID")]
    pub context: Option<String>,
    /// Reply to a task that stopped in `TASK_STATE_INPUT_REQUIRED`.
    #[arg(long, value_name = "TASK_ID")]
    pub task: Option<String>,
    /// Print the agent's reply as it is produced.
    #[arg(long, conflicts_with = "no_wait")]
    pub stream: bool,
    /// Return as soon as the task exists instead of waiting for the answer.
    ///
    /// Prints the task as JSON; follow it with `agent task get`.
    #[arg(long)]
    pub no_wait: bool,
    /// Print the protocol response as JSON instead of the reply text.
    #[arg(long)]
    pub raw: bool,
    /// Actor the message is attributed to (`metadata.memorylake.actorId`).
    #[arg(long, value_name = "ACTOR_ID")]
    pub actor: Option<String>,
    /// Project the agent reads from and writes memories to.
    #[arg(long, value_name = "PROJECT_ID")]
    pub project: Option<String>,
    /// Project the agent may read but not write. Repeatable.
    #[arg(long = "read-only-project", value_name = "PROJECT_ID")]
    pub read_only_projects: Vec<String>,
    /// Do not extract memories from this exchange.
    #[arg(long)]
    pub skip_memory: bool,
    /// Extra `metadata.memorylake` keys as a JSON object (e.g. `overrides`).
    ///
    /// Merged under the flags above; a key set both ways is rejected.
    #[arg(long, value_name = "JSON")]
    pub metadata_json: Option<String>,
    /// Workspace the agent is bound in.
    ///
    /// Defaults to the workspace remembered by `workspace use`.
    #[arg(long)]
    pub workspace: Option<String>,
}

/// `agent task` subcommands.
#[derive(Debug, Subcommand)]
pub enum TaskCommand {
    /// List an agent's tasks, newest first.
    List {
        /// Agent whose tasks to list.
        agent_id: String,
        /// Only tasks in this conversation thread.
        #[arg(long, value_name = "CONTEXT_ID")]
        context: Option<String>,
        /// Only tasks in this state (e.g. `TASK_STATE_WORKING`).
        #[arg(long, value_name = "STATE")]
        status: Option<String>,
        /// Tasks per page (1–100).
        #[arg(long, value_parser = clap::value_parser!(u32).range(1..=100))]
        page_size: Option<u32>,
        /// `nextPageToken` from the previous page.
        #[arg(long)]
        page_token: Option<String>,
        /// Only tasks whose status changed after this ISO 8601 instant.
        #[arg(long, value_name = "TIMESTAMP")]
        after: Option<String>,
        /// History messages to include per task.
        #[arg(long)]
        history_length: Option<u32>,
        /// Include each task's artifacts.
        #[arg(long)]
        artifacts: bool,
        /// Workspace the agent is bound in.
        ///
        /// Defaults to the workspace remembered by `workspace use`.
        #[arg(long)]
        workspace: Option<String>,
    },
    /// Get one task.
    Get {
        /// Agent that owns the task.
        agent_id: String,
        /// Task id.
        task_id: String,
        /// History messages to include.
        #[arg(long)]
        history_length: Option<u32>,
        /// Workspace the agent is bound in.
        ///
        /// Defaults to the workspace remembered by `workspace use`.
        #[arg(long)]
        workspace: Option<String>,
    },
    /// Cancel a running task.
    Cancel {
        /// Agent that owns the task.
        agent_id: String,
        /// Task id.
        task_id: String,
        /// Workspace the agent is bound in.
        ///
        /// Defaults to the workspace remembered by `workspace use`.
        #[arg(long)]
        workspace: Option<String>,
    },
    /// Rate a task's result.
    Feedback {
        /// Agent that owns the task.
        agent_id: String,
        /// Task id.
        task_id: String,
        /// Thumbs up or down.
        #[arg(long, value_enum)]
        rating: RatingArg,
        /// Free-text comment (at most 2000 characters).
        #[arg(long)]
        comment: Option<String>,
        /// Workspace the agent is bound in.
        ///
        /// Defaults to the workspace remembered by `workspace use`.
        #[arg(long)]
        workspace: Option<String>,
    },
}

/// `--rating` as spelled on the command line.
#[derive(Debug, Clone, Copy, PartialEq, Eq, ValueEnum)]
pub enum RatingArg {
    /// The result was good.
    Up,
    /// The result was bad.
    Down,
}

impl From<RatingArg> for Rating {
    fn from(rating: RatingArg) -> Self {
        match rating {
            RatingArg::Up => Self::Up,
            RatingArg::Down => Self::Down,
        }
    }
}

/// `agent card`.
pub fn run_card(client: &Client, workspace: &str, agent_id: &str) -> Result<()> {
    let card = get_agent_card(client, workspace, agent_id)
        .with_context(|| format!("get agent card of `{agent_id}` in workspace `{workspace}`"))?;
    print_json(&card)
}

/// `agent send`.
pub fn run_send(client: &Client, workspace: &str, args: SendArgs) -> Result<()> {
    let request = build_request(&args)?;
    let agent_id = &args.agent_id;

    if args.stream {
        let events = stream_message(client, workspace, agent_id, &request)
            .with_context(|| format!("stream message to agent `{agent_id}`"))?;
        return print_stream(events, args.raw);
    }

    let response = send_message(client, workspace, agent_id, &request)
        .with_context(|| format!("send message to agent `{agent_id}`"))?;
    if args.raw || args.no_wait {
        return print_json(&response);
    }
    print_reply(&response)
}

/// `agent task ...`.
pub fn run_task(client: &Client, workspace: &str, command: TaskCommand) -> Result<()> {
    match command {
        TaskCommand::List {
            agent_id,
            context,
            status,
            page_size,
            page_token,
            after,
            history_length,
            artifacts,
            workspace: _,
        } => {
            let params = ListTasksParams {
                context_id: context,
                status,
                page_size,
                page_token,
                history_length,
                status_timestamp_after: after,
                include_artifacts: artifacts.then_some(true),
            };
            let data = list_tasks(client, workspace, &agent_id, &params)
                .with_context(|| format!("list tasks of agent `{agent_id}`"))?;
            print_json(&data)
        }
        TaskCommand::Get {
            agent_id,
            task_id,
            history_length,
            workspace: _,
        } => {
            let data = get_task(client, workspace, &agent_id, &task_id, history_length)
                .with_context(|| format!("get task `{task_id}` of agent `{agent_id}`"))?;
            print_json(&data)
        }
        TaskCommand::Cancel {
            agent_id,
            task_id,
            workspace: _,
        } => {
            let data = cancel_task(client, workspace, &agent_id, &task_id)
                .with_context(|| format!("cancel task `{task_id}` of agent `{agent_id}`"))?;
            print_json(&data)
        }
        TaskCommand::Feedback {
            agent_id,
            task_id,
            rating,
            comment,
            workspace: _,
        } => {
            if let Some(comment) = &comment {
                let chars = comment.chars().count();
                if chars > FEEDBACK_COMMENT_MAX_CHARS {
                    bail!(
                        "--comment is {chars} characters; the limit is {FEEDBACK_COMMENT_MAX_CHARS}"
                    );
                }
            }
            let request = TaskFeedbackRequest {
                rating: rating.into(),
                comment,
            };
            let data = submit_task_feedback(client, workspace, &agent_id, &task_id, &request)
                .with_context(|| format!("rate task `{task_id}` of agent `{agent_id}`"))?;
            print_json(&data)
        }
    }
}

/// The `--workspace` flag of a `task` subcommand, for the shared resolver.
pub fn task_workspace_flag(command: &TaskCommand) -> Option<String> {
    match command {
        TaskCommand::List { workspace, .. }
        | TaskCommand::Get { workspace, .. }
        | TaskCommand::Cancel { workspace, .. }
        | TaskCommand::Feedback { workspace, .. } => workspace.clone(),
    }
}

/// Turn `send` flags into the request body.
pub fn build_request(args: &SendArgs) -> Result<SendMessageRequest> {
    let parts = build_parts(
        &args.text,
        args.message_json.as_deref(),
        args.message_file.as_deref(),
    )?;

    let configuration = SendConfiguration {
        return_immediately: args.no_wait.then_some(true),
        history_length: None,
    };

    let mut memorylake = MemorylakeExtension {
        actor_id: args.actor.clone(),
        read_write_project_id: args.project.clone(),
        read_only_project_ids: args.read_only_projects.clone(),
        skip_memory: args.skip_memory.then_some(true),
        extra: Map::new(),
    };
    if let Some(json) = &args.metadata_json {
        memorylake.extra = parse_extra_metadata(json, &memorylake)?;
    }

    Ok(SendMessageRequest {
        message: Message {
            role: ROLE_USER.into(),
            message_id: new_message_id(),
            parts,
            context_id: args.context.clone(),
            task_id: args.task.clone(),
        },
        configuration: (!configuration.is_empty()).then_some(configuration),
        metadata: (!memorylake.is_empty()).then_some(SendMetadata { memorylake }),
    })
}

/// A message id unique enough for the server to tell messages apart.
///
/// A2A only needs the id to be unique within its context; time, process and a
/// counter cover that without pulling in a UUID dependency.
fn new_message_id() -> String {
    static NEXT: AtomicU64 = AtomicU64::new(0);
    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_nanos())
        .unwrap_or_default();
    format!(
        "cli-{nanos:x}-{:x}-{}",
        std::process::id(),
        NEXT.fetch_add(1, Ordering::Relaxed)
    )
}

/// Build message parts from the mutually exclusive content flags.
///
/// Same rule as `conversation message append`: `--text` for the common case,
/// a JSON source for anything else, never both.
fn build_parts(
    texts: &[String],
    message_json: Option<&str>,
    message_file: Option<&Path>,
) -> Result<Vec<Value>> {
    let json_source = match (message_json, message_file) {
        (Some(_), Some(_)) => bail!("pass --message-json or --message-file, not both"),
        (Some(inline), None) => Some((inline.to_string(), "--message-json".to_string())),
        (None, Some(path)) => {
            let text = std::fs::read_to_string(path)
                .with_context(|| format!("read message file {}", path.display()))?;
            Some((text, format!("message file {}", path.display())))
        }
        (None, None) => None,
    };

    match (texts.is_empty(), json_source) {
        (false, Some((_, source))) => {
            bail!("--text and {source} both set the message; pass only one")
        }
        (true, None) => bail!(
            "a message is required: pass --text <TEXT>, \
             or --message-json / --message-file for non-text parts"
        ),
        (false, None) => Ok(texts.iter().map(text_part).collect()),
        (true, Some((json, source))) => parse_parts(&json, &source),
    }
}

/// Parse a JSON array of A2A parts. Each must be an object; what is inside is
/// the server's business.
fn parse_parts(json: &str, source: &str) -> Result<Vec<Value>> {
    let value: Value =
        serde_json::from_str(json).with_context(|| format!("parse JSON from {source}"))?;
    let Value::Array(items) = value else {
        bail!(
            "{source} must hold a JSON array of message parts\n\
             example: [{{\"text\": \"hello\"}}]"
        );
    };
    if items.is_empty() {
        bail!("{source} holds an empty array; a message needs at least one part");
    }
    for (index, item) in items.iter().enumerate() {
        if !item.is_object() {
            bail!("{source}: part {index} must be a JSON object");
        }
    }
    Ok(items)
}

/// Parse `--metadata-json` and refuse keys the dedicated flags already own.
fn parse_extra_metadata(json: &str, flags: &MemorylakeExtension) -> Result<Map<String, Value>> {
    let value: Value =
        serde_json::from_str(json).with_context(|| "parse JSON from --metadata-json")?;
    let Value::Object(extra) = value else {
        bail!(
            "--metadata-json must hold a JSON object, e.g. {{\"overrides\": {{\"model\": \"...\"}}}}"
        );
    };
    let owned = [
        ("actorId", flags.actor_id.is_some(), "--actor"),
        (
            "readWriteProjectId",
            flags.read_write_project_id.is_some(),
            "--project",
        ),
        (
            "readOnlyProjectIds",
            !flags.read_only_project_ids.is_empty(),
            "--read-only-project",
        ),
        ("skipMemory", flags.skip_memory.is_some(), "--skip-memory"),
    ];
    for (key, set_by_flag, flag) in owned {
        if extra.contains_key(key) {
            if set_by_flag {
                bail!("`{key}` is set both by {flag} and in --metadata-json; pass only one");
            }
            bail!("set `{key}` with {flag} rather than in --metadata-json");
        }
    }
    Ok(extra)
}

/// Print a blocking `send` response: the reply text, then where to continue.
fn print_reply(response: &Value) -> Result<()> {
    let task = response.get("task").unwrap_or(response);
    let text = reply_text(task);

    if text.is_empty() {
        // Nothing the CLI knows how to read as words; show everything rather
        // than nothing.
        print_json(response)?;
    } else {
        println!("{text}");
    }
    print_continuation(task);
    Ok(())
}

/// The agent's words in a finished task.
///
/// Artifacts hold the result of a completed task; the status message holds
/// what the agent said when it stopped for input. A bare `message` response
/// (no task) is read the same way.
fn reply_text(task: &Value) -> String {
    let mut text = String::new();
    if let Some(artifacts) = task.get("artifacts").and_then(Value::as_array) {
        for artifact in artifacts {
            append_parts_text(&mut text, artifact.get("parts"));
        }
    }
    if text.is_empty() {
        append_parts_text(&mut text, task.pointer("/status/message/parts"));
    }
    if text.is_empty() {
        // `{"message": {...}}` responses, or a task-less reply.
        append_parts_text(&mut text, task.get("parts"));
    }
    text
}

fn append_parts_text(out: &mut String, parts: Option<&Value>) {
    let Some(parts) = parts.and_then(Value::as_array) else {
        return;
    };
    for part in parts {
        if let Some(fragment) = part.get("text").and_then(Value::as_str) {
            out.push_str(fragment);
        }
    }
}

/// Tell the caller how to continue, on stderr so stdout stays the reply.
fn print_continuation(task: &Value) {
    let id = task.get("id").and_then(Value::as_str).unwrap_or("?");
    let context = task.get("contextId").and_then(Value::as_str).unwrap_or("?");
    let state = task
        .pointer("/status/state")
        .and_then(Value::as_str)
        .unwrap_or("?");
    eprintln!("task {id}  context {context}  state {state}");
    if state == TASK_STATE_INPUT_REQUIRED {
        eprintln!("the agent needs more input; reply with --task {id} --context {context}");
    }
}

/// Print a `message:stream` as it arrives.
///
/// Text mode writes each fragment the moment it lands and flushes, so a
/// caller watching the terminal sees the reply grow. Raw mode prints one
/// compact JSON document per event.
///
/// Production streams the reply twice: token by token in `statusUpdate`
/// events, then once more in full as the closing `artifactUpdate`. The
/// artifact is only printed when no status text arrived, so an agent that
/// streams nothing still shows its result and one that streams does not
/// repeat itself.
fn print_stream<I>(events: I, raw: bool) -> Result<()>
where
    I: Iterator<Item = memorylake_core::Result<Value>>,
{
    let stdout = std::io::stdout();
    let mut out = stdout.lock();
    let mut last_task: Option<Value> = None;
    let mut streamed_status_text = false;
    let mut artifact_text = String::new();

    for event in events {
        let event = event.context("read agent reply stream")?;
        if raw {
            writeln!(out, "{event}")?;
            out.flush()?;
            continue;
        }

        let fragment = status_text(&event);
        if !fragment.is_empty() {
            out.write_all(fragment.as_bytes())?;
            out.flush()?;
            streamed_status_text = true;
        }
        append_parts_text(
            &mut artifact_text,
            event.pointer("/artifactUpdate/artifact/parts"),
        );
        if let Some(task) = event_task_summary(&event) {
            last_task = Some(task);
        }
    }

    if raw {
        return Ok(());
    }
    if streamed_status_text {
        writeln!(out)?;
    } else if !artifact_text.is_empty() {
        writeln!(out, "{artifact_text}")?;
    }
    if let Some(task) = last_task {
        print_continuation(&task);
    }
    Ok(())
}

/// The words a stream event carries as the agent speaks them: the status
/// message of a `statusUpdate`, or a bare `message` response.
fn status_text(event: &Value) -> String {
    let mut text = String::new();
    append_parts_text(
        &mut text,
        event.pointer("/statusUpdate/status/message/parts"),
    );
    append_parts_text(&mut text, event.pointer("/message/parts"));
    text
}

/// The task id, context and state named by one stream event, in the shape
/// [`print_continuation`] reads.
fn event_task_summary(event: &Value) -> Option<Value> {
    if let Some(task) = event.get("task") {
        return Some(task.clone());
    }
    let update = event
        .get("statusUpdate")
        .or_else(|| event.get("artifactUpdate"))?;
    let mut summary = Map::new();
    if let Some(id) = update.get("taskId") {
        summary.insert("id".into(), id.clone());
    }
    if let Some(context) = update.get("contextId") {
        summary.insert("contextId".into(), context.clone());
    }
    if let Some(status) = update.get("status") {
        summary.insert("status".into(), status.clone());
    }
    Some(Value::Object(summary))
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn args(agent: &str) -> SendArgs {
        SendArgs {
            agent_id: agent.into(),
            text: vec![],
            message_json: None,
            message_file: None,
            context: None,
            task: None,
            stream: false,
            no_wait: false,
            raw: false,
            actor: None,
            project: None,
            read_only_projects: vec![],
            skip_memory: false,
            metadata_json: None,
            workspace: None,
        }
    }

    #[test]
    fn text_flags_become_text_parts_and_nothing_else_is_sent() {
        let mut a = args("agt-1");
        a.text = vec!["hello".into(), "world".into()];
        let request = build_request(&a).unwrap();
        let body = serde_json::to_value(&request).unwrap();
        assert_eq!(body["message"]["role"], "ROLE_USER");
        assert_eq!(
            body["message"]["parts"],
            json!([{"text": "hello"}, {"text": "world"}])
        );
        assert!(
            body["message"]["messageId"]
                .as_str()
                .unwrap()
                .starts_with("cli-")
        );
        assert!(body.get("configuration").is_none(), "{body}");
        assert!(body.get("metadata").is_none(), "{body}");
    }

    #[test]
    fn message_ids_differ_between_calls() {
        assert_ne!(new_message_id(), new_message_id());
    }

    #[test]
    fn scope_flags_land_under_metadata_memorylake() {
        let mut a = args("agt-1");
        a.text = vec!["hi".into()];
        a.actor = Some("act-1".into());
        a.project = Some("proj-1".into());
        a.read_only_projects = vec!["proj-2".into(), "proj-3".into()];
        a.skip_memory = true;
        a.context = Some("ctx".into());
        a.task = Some("run-1".into());
        a.no_wait = true;
        let body = serde_json::to_value(build_request(&a).unwrap()).unwrap();
        assert_eq!(
            body["metadata"]["memorylake"],
            json!({
                "actorId": "act-1",
                "readWriteProjectId": "proj-1",
                "readOnlyProjectIds": ["proj-2", "proj-3"],
                "skipMemory": true
            })
        );
        assert_eq!(body["configuration"], json!({"returnImmediately": true}));
        assert_eq!(body["message"]["contextId"], "ctx");
        assert_eq!(body["message"]["taskId"], "run-1");
    }

    #[test]
    fn metadata_json_is_merged_but_may_not_shadow_a_flag() {
        let mut a = args("agt-1");
        a.text = vec!["hi".into()];
        a.metadata_json = Some(r#"{"overrides":{"model":"x"}}"#.into());
        let body = serde_json::to_value(build_request(&a).unwrap()).unwrap();
        assert_eq!(
            body["metadata"]["memorylake"],
            json!({"overrides": {"model": "x"}})
        );

        a.metadata_json = Some(r#"{"skipMemory":true}"#.into());
        let err = build_request(&a).unwrap_err().to_string();
        assert!(err.contains("--skip-memory"), "{err}");

        a.skip_memory = true;
        let err = build_request(&a).unwrap_err().to_string();
        assert!(err.contains("both"), "{err}");

        a.metadata_json = Some("[]".into());
        let err = build_request(&a).unwrap_err().to_string();
        assert!(err.contains("JSON object"), "{err}");
    }

    #[test]
    fn message_content_is_required_and_exclusive() {
        let a = args("agt-1");
        let err = build_request(&a).unwrap_err().to_string();
        assert!(err.contains("--text"), "{err}");

        let mut a = args("agt-1");
        a.text = vec!["hi".into()];
        a.message_json = Some(r#"[{"text":"x"}]"#.into());
        let err = build_request(&a).unwrap_err().to_string();
        assert!(err.contains("pass only one"), "{err}");

        let mut a = args("agt-1");
        a.message_json =
            Some(r#"[{"text":"x"},{"url":"https://e/x.png","mediaType":"image/png"}]"#.into());
        let body = serde_json::to_value(build_request(&a).unwrap()).unwrap();
        assert_eq!(body["message"]["parts"].as_array().unwrap().len(), 2);

        a.message_json = Some(r#"{"text":"x"}"#.into());
        let err = build_request(&a).unwrap_err().to_string();
        assert!(err.contains("JSON array"), "{err}");

        a.message_json = Some("[]".into());
        let err = build_request(&a).unwrap_err().to_string();
        assert!(err.contains("empty array"), "{err}");

        a.message_json = Some(r#"["x"]"#.into());
        let err = build_request(&a).unwrap_err().to_string();
        assert!(err.contains("part 0"), "{err}");
    }

    #[test]
    fn reply_text_prefers_artifacts_then_the_status_message() {
        let completed = json!({
            "id": "run-1",
            "status": {"state": "TASK_STATE_COMPLETED"},
            "artifacts": [{"parts": [{"text": "PO"}, {"text": "NG"}]}],
            "history": [{"role": "ROLE_AGENT", "parts": [{"text": "ignored"}]}]
        });
        assert_eq!(reply_text(&completed), "PONG");

        let input_required = json!({
            "id": "run-2",
            "status": {
                "state": "TASK_STATE_INPUT_REQUIRED",
                "message": {"parts": [{"text": "Which file?"}]}
            },
            "artifacts": []
        });
        assert_eq!(reply_text(&input_required), "Which file?");

        let bare_message = json!({"role": "ROLE_AGENT", "parts": [{"text": "hi"}]});
        assert_eq!(reply_text(&bare_message), "hi");

        assert_eq!(reply_text(&json!({"id": "run-3"})), "");
    }

    #[test]
    fn stream_events_yield_their_text_and_task_summary() {
        let first = json!({"task": {"id": "run-1", "contextId": "ctx", "status": {"state": "TASK_STATE_WORKING"}}});
        assert_eq!(status_text(&first), "");
        assert_eq!(event_task_summary(&first).unwrap()["id"], "run-1");

        let update = json!({"statusUpdate": {
            "taskId": "run-1", "contextId": "ctx",
            "status": {"state": "TASK_STATE_WORKING", "message": {"parts": [{"text": "1"}, {"text": "\n"}]}}
        }});
        assert_eq!(status_text(&update), "1\n");
        let summary = event_task_summary(&update).unwrap();
        assert_eq!(summary["id"], "run-1");
        assert_eq!(summary["contextId"], "ctx");
        assert_eq!(summary["status"]["state"], "TASK_STATE_WORKING");

        let artifact = json!({"artifactUpdate": {"taskId": "run-1", "artifact": {"parts": [{"text": "done"}]}}});
        assert_eq!(
            status_text(&artifact),
            "",
            "artifact text is the consolidated reply, not a streamed fragment"
        );
        assert_eq!(event_task_summary(&artifact).unwrap()["id"], "run-1");

        assert!(event_task_summary(&json!({"unknown": {}})).is_none());
    }
}
