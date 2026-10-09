//! List the fact changes a conversation produced
//! (`GET .../memories/conversations/{id}/fact-actions`).

use serde::{Deserialize, Serialize};
use serde_json::{Map, Value};

use crate::client::Client;
use crate::error::Result;

use super::path::fact_actions_path;

/// Largest `page_size` the endpoint accepts.
pub const FACT_ACTIONS_MAX_PAGE_SIZE: u32 = 100;

/// Whose memory to report changes for.
///
/// The endpoint needs exactly one owner and answers for that owner alone. An
/// agent's memory is that of the actor backing it, so an agent is reached
/// through [`FactOwner::Actor`] with the agent's `actor_id`; `owner_type=agent`
/// is rejected (measured 2026-10-09).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum FactOwner {
    /// A project's memory.
    Project(String),
    /// An actor's memory.
    Actor(String),
}

impl FactOwner {
    /// `owner_type` wire value.
    pub fn owner_type(&self) -> &'static str {
        match self {
            Self::Project(_) => "project",
            Self::Actor(_) => "actor",
        }
    }

    /// `owner_id` wire value.
    pub fn owner_id(&self) -> &str {
        match self {
            Self::Project(id) | Self::Actor(id) => id,
        }
    }
}

/// Query parameters for listing fact changes.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ListFactActionsParams {
    /// Whose memory to list changes for.
    pub owner: FactOwner,
    /// Keep only the changes drawn from this message (an internal id).
    pub message_id: Option<String>,
    /// Treat the conversation id as its caller-defined `custom_id`. The
    /// workspace, owner and message ids stay internal ids.
    pub by_custom_id: bool,
    /// Page size, 1–[`FACT_ACTIONS_MAX_PAGE_SIZE`]. The server defaults to 20.
    pub page_size: Option<u32>,
    /// Continuation token from a previous page.
    pub continuation_token: Option<String>,
}

impl ListFactActionsParams {
    /// Params naming only the owner.
    pub fn for_owner(owner: FactOwner) -> Self {
        Self {
            owner,
            message_id: None,
            by_custom_id: false,
            page_size: None,
            continuation_token: None,
        }
    }

    fn to_query(&self) -> Vec<(&'static str, String)> {
        let mut query = vec![
            ("owner_type", self.owner.owner_type().to_string()),
            ("owner_id", self.owner.owner_id().to_string()),
        ];
        if let Some(message_id) = &self.message_id {
            query.push(("message_id", message_id.clone()));
        }
        if self.by_custom_id {
            query.push(("by_custom_id", "true".to_string()));
        }
        if let Some(page_size) = self.page_size {
            query.push(("page_size", page_size.to_string()));
        }
        if let Some(token) = &self.continuation_token {
            query.push(("continuation_token", token.clone()));
        }
        query
    }
}

/// One change made to a fact.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct FactAction {
    /// `ADD`, `UPDATE` or `FORGET`. A string so a new kind cannot fail the
    /// decode.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub event: Option<String>,
    /// Id of this change record.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub history_id: Option<String>,
    /// Id of the fact that changed.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub fact_id: Option<String>,
    /// The fact before the change; absent for `ADD`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub old_fact: Option<String>,
    /// The fact after the change; absent for `FORGET`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub new_fact: Option<String>,
    /// When the change was recorded (ISO 8601).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub changed_at: Option<String>,
    /// Fields the CLI does not model yet.
    #[serde(flatten)]
    pub extra: Map<String, Value>,
}

/// One page of fact changes, newest first.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct FactActionList {
    /// Changes on this page.
    #[serde(default)]
    pub items: Vec<FactAction>,
    /// Count across all pages, when the server provides it.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub total: Option<u64>,
    /// Token for the next page, if any.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub continuation_token: Option<String>,
}

/// List the facts a conversation added, updated or forgot in one owner's
/// memory.
///
/// Memory is extracted in the background, so an empty page can mean "nothing
/// processed yet" as well as "nothing learned"; poll if the difference
/// matters.
pub fn list_fact_actions(
    client: &Client,
    workspace_id: &str,
    conversation_id: &str,
    params: &ListFactActionsParams,
) -> Result<FactActionList> {
    client.get_data(
        &fact_actions_path(workspace_id, conversation_id),
        &params.to_query(),
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_owner_is_always_sent_and_nothing_else_by_default() {
        let params = ListFactActionsParams::for_owner(FactOwner::Actor("actor-1".into()));
        assert_eq!(
            params.to_query(),
            vec![
                ("owner_type", "actor".to_string()),
                ("owner_id", "actor-1".to_string()),
            ]
        );
    }

    #[test]
    fn every_filter_renders_under_its_documented_name() {
        let params = ListFactActionsParams {
            owner: FactOwner::Project("proj-1".into()),
            message_id: Some("conv-entry-1".into()),
            by_custom_id: true,
            page_size: Some(5),
            continuation_token: Some("tok".into()),
        };
        assert_eq!(
            params.to_query(),
            vec![
                ("owner_type", "project".to_string()),
                ("owner_id", "proj-1".to_string()),
                ("message_id", "conv-entry-1".to_string()),
                ("by_custom_id", "true".to_string()),
                ("page_size", "5".to_string()),
                ("continuation_token", "tok".to_string()),
            ]
        );
    }

    #[test]
    fn the_measured_page_decodes() {
        let page: FactActionList = serde_json::from_str(
            r#"{"items":[{"event":"ADD","history_id":"facthist-1","fact_id":"fact-1",
                "new_fact":"The person lives in Lisbon.","changed_at":"2026-10-09T07:44:21.381767Z"}],
                "total":5,"continuation_token":"tok"}"#,
        )
        .expect("decode");
        assert_eq!(page.items[0].event.as_deref(), Some("ADD"));
        assert_eq!(page.items[0].old_fact, None);
        assert_eq!(page.total, Some(5));
        assert_eq!(page.continuation_token.as_deref(), Some("tok"));
    }

    #[test]
    fn an_empty_page_decodes() {
        let page: FactActionList = serde_json::from_str("{}").expect("decode");
        assert!(page.items.is_empty());
    }
}
