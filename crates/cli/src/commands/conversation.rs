//! `memorylake conversation` / `conv` commands.
//!
//! A conversation belongs to a workspace and writes what it learns into
//! exactly one project, so `create`, `get`, `list`, `delete` and `cook-status`
//! all take `--workspace`. The message subcommands deliberately do not: the
//! API addresses messages by conversation id alone.

mod input;
mod wait;

use std::path::PathBuf;
use std::time::Duration;

use anyhow::{Context, Result, bail};
use clap::{Subcommand, ValueEnum};
use memorylake_core::api::conversations::{
    AppendMessageRequest, ConversationKind, CreateConversationRequest, ListConversationsParams,
    ListMessagesParams, append_message, create_conversation, delete_conversation, get_conversation,
    get_conversation_by_custom_id, get_cook_status, get_cook_status_by_custom_id,
    list_conversations, list_messages,
};
use memorylake_core::{Client, Paths, ResolveOverrides, resolve};

use super::search::{IdList, parse_id_list};
use input::{build_content, collect_metadata, parse_metadata_pair};
use wait::{ApiCookPoller, DEFAULT_WAIT_TIMEOUT_SECS, WaitOutcome, wait_for_cook};

/// Conversation kind as spelled on the command line.
///
/// Spelled as the API spells it, and closed and case-sensitive like
/// `actor --type`: a typo is worth rejecting here rather than paying a round
/// trip for.
#[derive(Debug, Clone, Copy, PartialEq, Eq, ValueEnum)]
pub enum KindArg {
    /// A conversation between two participants.
    #[value(name = "DIRECT")]
    Direct,
    /// A conversation with several participants.
    #[value(name = "GROUP")]
    Group,
}

impl From<KindArg> for ConversationKind {
    fn from(kind: KindArg) -> Self {
        match kind {
            KindArg::Direct => Self::Direct,
            KindArg::Group => Self::Group,
        }
    }
}

/// `conversation` subcommands.
#[derive(Debug, Subcommand)]
pub enum ConversationCommand {
    /// Create a conversation.
    ///
    /// `--project` is the scope the conversation may read context from and
    /// write to; the caller needs `mem_add` and `doc_add` on it, and the API
    /// accepts exactly one such project today. It does not decide where the
    /// facts extracted from the conversation end up — the server attributes
    /// each of those to an actor or a project on its own.
    Create {
        /// Workspace id to create the conversation in.
        #[arg(long)]
        workspace: String,
        /// Caller-defined id. Must be unique within the project.
        #[arg(long)]
        custom_id: String,
        /// Project scope this conversation may read from and write to.
        #[arg(long)]
        project: String,
        /// Conversation title.
        #[arg(long)]
        name: Option<String>,
        /// Whether the conversation is DIRECT or GROUP.
        #[arg(long, value_enum, default_value = "DIRECT")]
        kind: KindArg,
        /// Actors participating in the conversation (comma-separated).
        #[arg(long, value_name = "IDS", value_parser = parse_id_list)]
        actors: Option<IdList>,
        /// Metadata entry, repeatable (`--metadata key=value`).
        #[arg(long = "metadata", value_name = "KEY=VALUE", value_parser = parse_metadata_pair)]
        metadata: Vec<(String, String)>,
    },
    /// List conversations in a workspace.
    ///
    /// The API offers no server-side filter here — not by project, not by
    /// actor — so this returns the workspace's conversations as they come.
    List {
        /// Workspace id that owns the conversations.
        #[arg(long)]
        workspace: String,
        /// Number of items per page.
        #[arg(long)]
        page_size: Option<u32>,
        /// Continuation token from a previous response.
        #[arg(long)]
        continuation_token: Option<String>,
    },
    /// Get a single conversation.
    Get {
        /// Workspace id that owns the conversation.
        #[arg(long)]
        workspace: String,
        /// Conversation id (or custom_id when `--by-custom-id` is set).
        id: String,
        /// Treat the positional argument as a caller-defined custom_id.
        #[arg(long)]
        by_custom_id: bool,
    },
    /// Permanently delete a conversation and every message in it.
    ///
    /// This cannot be undone, and the command does not ask for confirmation.
    /// The endpoint takes a conversation id only; resolve a custom_id with
    /// `conversation get --by-custom-id` first.
    Delete {
        /// Workspace id that owns the conversation.
        #[arg(long)]
        workspace: String,
        /// Conversation id.
        id: String,
    },
    /// Report whether a conversation's memory has finished building.
    ///
    /// Messages are stored immediately but turned into memory in the
    /// background, so a conversation is not searchable the moment it is
    /// appended to. Poll this to find out when it is — with a timeout of your
    /// own, because not every conversation reaches a finished state.
    CookStatus {
        /// Workspace id that owns the conversation.
        #[arg(long)]
        workspace: String,
        /// Conversation id (or custom_id when `--by-custom-id` is set).
        id: String,
        /// Treat the positional argument as a caller-defined custom_id.
        #[arg(long)]
        by_custom_id: bool,
    },
    /// Append and list the messages of a conversation.
    #[command(visible_alias = "msg")]
    Message {
        #[command(subcommand)]
        command: MessageCommand,
    },
}

/// `conversation message` subcommands.
///
/// No `--workspace`: the messages endpoints are addressed by conversation id
/// alone. `append --wait` is the one exception, because the status it polls
/// lives on the workspace-scoped `cook-status` endpoint.
#[derive(Debug, Subcommand)]
pub enum MessageCommand {
    /// Append a message to a conversation.
    ///
    /// Content comes either from one or more `--text` flags, one `TEXT` block
    /// each, or from a JSON array of blocks (`--content-json` /
    /// `--content-file`) for the types `--text` cannot express: `FILE`,
    /// `IMAGE`, `THINKING`, `TOOL_USE`, `TOOL_RESULT`.
    ///
    /// Appends to one conversation are serialized server-side, so two at once
    /// leave one caller with a 409. Retrying is safe: the same `--custom-id`
    /// returns the message created the first time instead of duplicating it.
    /// After a 409, re-read `message list` and retry with `--parent` set to
    /// the current last message.
    ///
    /// The message is stored the moment this returns, but the facts drawn from
    /// it are extracted in the background. Pass `--wait` (with `--workspace`)
    /// to keep polling until the conversation reports its memory finished.
    Append {
        /// Conversation id to append to.
        conversation: String,
        /// Actor sending the message.
        #[arg(long)]
        actor: String,
        /// Caller-defined id. Must be unique within the conversation, and
        /// makes a retry idempotent.
        #[arg(long)]
        custom_id: String,
        /// Text content, repeatable; each becomes one TEXT block.
        #[arg(long = "text", value_name = "TEXT")]
        texts: Vec<String>,
        /// Content blocks as an inline JSON array.
        #[arg(long, value_name = "JSON")]
        content_json: Option<String>,
        /// Content blocks read from a JSON file.
        #[arg(long, value_name = "PATH")]
        content_file: Option<PathBuf>,
        /// Message this one replies to. Defaults to the conversation's latest.
        #[arg(long = "parent", value_name = "MESSAGE_ID")]
        parent_message_id: Option<String>,
        /// Message timestamp (ISO 8601). Defaults to server time.
        #[arg(long)]
        timestamp: Option<String>,
        /// Metadata entry, repeatable (`--metadata key=value`).
        #[arg(long = "metadata", value_name = "KEY=VALUE", value_parser = parse_metadata_pair)]
        metadata: Vec<(String, String)>,
        /// Poll until the conversation's memory finishes building.
        ///
        /// Requires --workspace: the status lives on a workspace-scoped
        /// endpoint. The wait covers the whole conversation, not this message
        /// alone, so a concurrent writer can keep it unfinished.
        #[arg(long, requires = "workspace")]
        wait: bool,
        /// Workspace that owns the conversation. Only needed with --wait.
        #[arg(long)]
        workspace: Option<String>,
        /// Seconds to keep polling. Only meaningful with `--wait`.
        ///
        /// Giving up does not undo the append or stop the processing; both
        /// carry on server-side.
        #[arg(long, default_value_t = DEFAULT_WAIT_TIMEOUT_SECS, value_name = "SECS")]
        timeout: u64,
    },
    /// List the messages in a conversation.
    List {
        /// Conversation id to list.
        conversation: String,
        /// Number of items per page.
        #[arg(long)]
        page_size: Option<u32>,
        /// Continuation token from a previous response.
        #[arg(long)]
        continuation_token: Option<String>,
    },
}

/// Execute a `conversation` subcommand.
pub fn run(
    command: ConversationCommand,
    profile: Option<String>,
    base_url: Option<String>,
) -> Result<()> {
    let paths = Paths::default_home().context("resolve MemoryLake config paths")?;
    let runtime = resolve(&paths, &ResolveOverrides { profile, base_url })
        .context("resolve API credentials")?;
    let client = Client::new(&runtime.base_url, &runtime.api_key).context("build API client")?;

    match command {
        ConversationCommand::Create {
            workspace,
            custom_id,
            project,
            name,
            kind,
            actors,
            metadata,
        } => {
            let request = CreateConversationRequest {
                custom_id,
                kind: kind.into(),
                rw_project_ids: vec![project],
                name,
                actor_ids: actors.map(|list| list.0).unwrap_or_default(),
                metadata: collect_metadata(metadata),
            };
            let data = create_conversation(&client, &workspace, &request)
                .context("create conversation")?;
            println!("{}", serde_json::to_string_pretty(&data)?);
        }
        ConversationCommand::List {
            workspace,
            page_size,
            continuation_token,
        } => {
            let params = ListConversationsParams {
                page_size,
                continuation_token,
            };
            let data =
                list_conversations(&client, &workspace, &params).context("list conversations")?;
            println!("{}", serde_json::to_string_pretty(&data)?);
        }
        ConversationCommand::Get {
            workspace,
            id,
            by_custom_id,
        } => {
            let data = if by_custom_id {
                get_conversation_by_custom_id(&client, &workspace, &id)
            } else {
                get_conversation(&client, &workspace, &id)
            }
            .with_context(|| format!("get conversation `{id}`"))?;
            println!("{}", serde_json::to_string_pretty(&data)?);
        }
        ConversationCommand::Delete { workspace, id } => {
            delete_conversation(&client, &workspace, &id)
                .with_context(|| format!("delete conversation `{id}`"))?;
            println!("deleted conversation {id}");
        }
        ConversationCommand::CookStatus {
            workspace,
            id,
            by_custom_id,
        } => {
            let data = if by_custom_id {
                get_cook_status_by_custom_id(&client, &workspace, &id)
            } else {
                get_cook_status(&client, &workspace, &id)
            }
            .with_context(|| format!("get cook status of conversation `{id}`"))?;
            println!("{}", serde_json::to_string_pretty(&data)?);
        }
        ConversationCommand::Message { command } => run_message(&client, command)?,
    }

    Ok(())
}

/// Execute a `conversation message` subcommand.
fn run_message(client: &Client, command: MessageCommand) -> Result<()> {
    match command {
        MessageCommand::Append {
            conversation,
            actor,
            custom_id,
            texts,
            content_json,
            content_file,
            parent_message_id,
            timestamp,
            metadata,
            wait,
            workspace,
            timeout,
        } => {
            let content = build_content(texts, content_json, content_file.as_deref())?;
            let request = AppendMessageRequest {
                actor_id: actor,
                custom_id,
                content,
                parent_message_id,
                timestamp,
                metadata: collect_metadata(metadata),
            };
            let data = append_message(client, &conversation, &request)
                .with_context(|| format!("append message to conversation `{conversation}`"))?;
            // Printed before any wait: the message is already stored, and the
            // caller must not have to choose between seeing it and seeing a
            // timeout.
            println!("{}", serde_json::to_string_pretty(&data)?);

            if wait {
                // clap's `requires` guarantees the workspace is present.
                let workspace = workspace.context("--wait requires --workspace")?;
                let poller = ApiCookPoller::new(client, &workspace, &conversation);
                let outcome = wait_for_cook(&poller, Duration::from_secs(timeout))?;
                if outcome == WaitOutcome::TimedOut {
                    bail!(
                        "conversation `{conversation}` was still building its memory when \
                         --timeout elapsed ({timeout}s); the message was appended and \
                         processing continues on the server"
                    );
                }
            }
        }
        MessageCommand::List {
            conversation,
            page_size,
            continuation_token,
        } => {
            let params = ListMessagesParams {
                page_size,
                continuation_token,
            };
            let data = list_messages(client, &conversation, &params)
                .with_context(|| format!("list messages of conversation `{conversation}`"))?;
            println!("{}", serde_json::to_string_pretty(&data)?);
        }
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn kind_args_map_onto_the_wire_values() {
        assert_eq!(ConversationKind::from(KindArg::Direct).as_wire(), "DIRECT");
        assert_eq!(ConversationKind::from(KindArg::Group).as_wire(), "GROUP");
    }
}
