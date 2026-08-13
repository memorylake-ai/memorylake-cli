//! Conversations v3 API
//! (`.../workspaces/{id}/memories/conversations` and `.../conversations/{id}/messages`).
//!
//! A conversation is an ordered log of messages that the server turns into
//! memory in the background. It belongs to a workspace and writes what it
//! learns into exactly one read-write project, named at creation time.
//!
//! Two things follow from that and shape this module:
//!
//! * **Two address spaces.** The conversation itself is addressed under its
//!   workspace; its messages are addressed by conversation id alone, with no
//!   workspace segment. Both are spelled out in `path.rs`.
//! * **Memory lags messages.** An appended message is stored immediately but
//!   is not searchable until the server has processed it, so
//!   [`get_cook_status`] exists to tell the two states apart.
//!
//! Appends within one conversation are serialized: concurrent ones leave one
//! caller with a 409, recoverable because every message carries a caller-chosen
//! `custom_id` that makes a retry idempotent.

mod append_message;
mod cook_status;
mod create;
mod delete;
mod get;
mod list;
mod list_messages;
mod path;
mod types;

pub use append_message::{AppendMessageRequest, append_message};
pub use cook_status::{get_cook_status, get_cook_status_by_custom_id};
pub use create::{CreateConversationRequest, create_conversation};
pub use delete::delete_conversation;
pub use get::{get_conversation, get_conversation_by_custom_id};
pub use list::{ConversationList, ListConversationsParams, list_conversations};
pub use list_messages::{ListMessagesParams, MessageList, list_messages};
pub use types::{
    BLOCK_TYPES, ContentBlock, Conversation, ConversationKind, CookStatus, Message, Metadata,
    text_block,
};
