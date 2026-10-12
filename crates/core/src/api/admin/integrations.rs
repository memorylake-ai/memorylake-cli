//! IM channel integrations (`/admin/v1/integrations`): a DingTalk, WeCom,
//! Feishu, or Slack bot connected to an agent.
//!
//! Credentials are write-only: they are sent on create and on a rotation,
//! and never come back. [`IntegrationCredentials`] keeps them out of debug
//! output, and the client redacts them from trace logs.

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize, Serializer};
use serde_json::{Map, Value};

use crate::client::Client;
use crate::error::Result;

use super::path;
use super::types::{Page, push_opt, push_page_query};
use super::{EmptyData, idempotency_headers};

/// The IM platforms an integration can connect to.
pub const CHANNELS: [&str; 4] = ["dingtalk", "wecom", "feishu", "slack"];

/// Most open-data datasets one integration may use.
pub const MAX_INTEGRATION_DATASETS: usize = 64;

/// Longest dataset id, in bytes.
pub const MAX_INTEGRATION_DATASET_ID: usize = 128;

/// Longest display name.
pub const MAX_INTEGRATION_NAME: usize = 255;

/// The two credential fields a channel takes: the bot's identifier (not a
/// secret, fixed for the integration's life) and its secret.
pub fn credential_keys(channel: &str) -> Option<[&'static str; 2]> {
    match channel {
        "dingtalk" => Some(["appKey", "appSecret"]),
        "feishu" => Some(["appId", "appSecret"]),
        "slack" => Some(["botToken", "appToken"]),
        "wecom" => Some(["botId", "secret"]),
        _ => None,
    }
}

/// A bot's credentials, keyed by the platform's own field names.
///
/// Debug output names the keys but never the values.
#[derive(Clone, PartialEq, Eq)]
pub struct IntegrationCredentials(BTreeMap<String, String>);

impl IntegrationCredentials {
    /// Parse a JSON object of string values, e.g.
    /// `{"botToken":"xoxb-…","appToken":"xapp-…"}`.
    pub fn from_json(json: &str) -> std::result::Result<Self, String> {
        let value: Value = serde_json::from_str(json)
            .map_err(|err| format!("credentials are not valid JSON: {err}"))?;
        let Value::Object(object) = value else {
            return Err("credentials must be a JSON object of string values".to_string());
        };
        let mut fields = BTreeMap::new();
        for (key, value) in object {
            match value {
                Value::String(s) if !s.trim().is_empty() => {
                    fields.insert(key, s);
                }
                Value::String(_) => return Err(format!("credential `{key}` is empty")),
                _ => return Err(format!("credential `{key}` must be a string")),
            }
        }
        if fields.is_empty() {
            return Err("credentials are empty".to_string());
        }
        Ok(Self(fields))
    }

    /// The field names given.
    pub fn keys(&self) -> impl Iterator<Item = &str> {
        self.0.keys().map(String::as_str)
    }

    /// Check that these are exactly the fields `channel` takes.
    pub fn check_complete_for(&self, channel: &str) -> std::result::Result<(), String> {
        let Some(expected) = credential_keys(channel) else {
            return Err(format!("unknown channel `{channel}`"));
        };
        let given: Vec<&str> = self.keys().collect();
        let mut wanted = expected.to_vec();
        wanted.sort_unstable();
        if given != wanted {
            return Err(format!(
                "{channel} credentials are exactly {} and {}; got {}",
                expected[0],
                expected[1],
                given.join(", ")
            ));
        }
        Ok(())
    }

    /// Check that every field belongs to some channel, for a rotation whose
    /// channel is only known to the server.
    pub fn check_known(&self) -> std::result::Result<(), String> {
        let known: Vec<&str> = CHANNELS
            .iter()
            .flat_map(|c| credential_keys(c).unwrap_or_default())
            .collect();
        for key in self.keys() {
            if !known.contains(&key) {
                return Err(format!(
                    "`{key}` is not a credential field; the fields are {}",
                    known.join(", ")
                ));
            }
        }
        Ok(())
    }
}

impl std::fmt::Debug for IntegrationCredentials {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let keys: Vec<&str> = self.keys().collect();
        write!(f, "IntegrationCredentials({keys:?}: <redacted>)")
    }
}

impl Serialize for IntegrationCredentials {
    fn serialize<S: Serializer>(&self, serializer: S) -> std::result::Result<S::Ok, S::Error> {
        self.0.serialize(serializer)
    }
}

/// Channel-specific settings.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct IntegrationSettings {
    /// DingTalk only: AI card template id; `""` means plain messages.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub card_template_id: Option<String>,
    /// Fields the CLI does not model yet.
    #[serde(flatten)]
    pub extra: Map<String, Value>,
}

/// An IM channel integration.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Integration {
    /// Identifier.
    pub id: String,
    /// `dingtalk`, `wecom`, `feishu`, or `slack`.
    pub channel: String,
    /// Display name.
    pub name: String,
    /// The agent answering messages.
    pub agent_id: String,
    /// The workspace, fixed at creation.
    pub workspace_id: String,
    /// Where memories extracted from IM conversations go.
    pub read_write_project_id: String,
    /// Additional projects searched, read only.
    #[serde(default)]
    pub read_only_project_ids: Option<Vec<String>>,
    /// Open-data datasets the agent may use.
    #[serde(default)]
    pub dataset_ids: Option<Vec<String>>,
    /// The bot's identifier on the platform. Not a secret.
    pub external_app_id: String,
    /// Whether credentials are stored; they are never returned.
    pub credentials_configured: bool,
    /// Channel-specific settings.
    pub settings: IntegrationSettings,
    /// Messages handled per day; 0 = unlimited.
    pub daily_message_limit: i64,
    /// Whether the bot should be connected.
    pub enabled: bool,
    /// `pending`, `connected`, `error`, or `disabled`.
    pub status: String,
    /// Display reason when `status` is `error`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub status_error: Option<String>,
    /// Stable reason code when `status` is `error`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub status_error_code: Option<String>,
    /// Unix seconds.
    pub created_at: i64,
    /// Unix seconds.
    pub updated_at: i64,
    /// Fields the CLI does not model yet.
    #[serde(flatten)]
    pub extra: Map<String, Value>,
}

/// Request body connecting a bot to an agent.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct CreateIntegrationRequest {
    /// `dingtalk`, `wecom`, `feishu`, or `slack`.
    pub channel: String,
    /// The workspace; cannot change later.
    pub workspace_id: String,
    /// Display name, 1–255 characters.
    pub name: String,
    /// The agent answering messages.
    pub agent_id: String,
    /// Where extracted memories go.
    pub read_write_project_id: String,
    /// Additional projects searched, read only.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub read_only_project_ids: Option<Vec<String>>,
    /// Open-data datasets, at most 64.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub dataset_ids: Option<Vec<String>>,
    /// Messages per day; omitted or 0 = unlimited.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub daily_message_limit: Option<i64>,
    /// Channel-specific settings.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub settings: Option<IntegrationSettings>,
    /// The bot's credentials.
    pub credentials: IntegrationCredentials,
}

/// Request body changing an integration. Only the fields given change;
/// credentials left out are kept.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize)]
pub struct UpdateIntegrationRequest {
    /// New display name.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,
    /// Switch the answering agent; ongoing conversations start over.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub agent_id: Option<String>,
    /// New project for extracted memories.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub read_write_project_id: Option<String>,
    /// Replace the read-only projects; `[]` removes them.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub read_only_project_ids: Option<Vec<String>>,
    /// Replace the datasets; `[]` removes them.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub dataset_ids: Option<Vec<String>>,
    /// Messages per day; 0 = unlimited.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub daily_message_limit: Option<i64>,
    /// Replace the channel-specific settings.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub settings: Option<IntegrationSettings>,
    /// Connect or disconnect the bot.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub enabled: Option<bool>,
    /// New credential values; a secret can be rotated alone.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub credentials: Option<IntegrationCredentials>,
}

/// Query parameters for listing integrations.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct ListIntegrationsParams {
    /// Page size.
    pub page_size: Option<u32>,
    /// Continuation token from a previous page.
    pub continuation_token: Option<String>,
    /// Only this workspace's integrations.
    pub workspace_id: Option<String>,
    /// Only integrations answered by this agent.
    pub agent_id: Option<String>,
}

/// List the integrations in the workspaces the caller may manage, newest
/// first.
pub fn list_integrations(
    client: &Client,
    params: &ListIntegrationsParams,
) -> Result<Page<Integration>> {
    let mut query = Vec::new();
    push_page_query(
        &mut query,
        params.page_size,
        params.continuation_token.as_deref(),
    );
    push_opt(&mut query, "workspace_id", params.workspace_id.as_ref());
    push_opt(&mut query, "agent_id", params.agent_id.as_ref());
    client.get_data(path::INTEGRATIONS, &query)
}

/// Return one integration. Poll it after a create or a credential change to
/// follow `status` from `pending` to `connected` or `error`.
pub fn get_integration(client: &Client, id: &str) -> Result<Integration> {
    client.get_data(&path::integration(id), &[])
}

/// Connect a bot. DingTalk, Feishu, and Slack credentials are verified with
/// the platform first; WeCom's are checked by connecting, so follow
/// `status`. A bot can be connected only once.
pub fn create_integration(
    client: &Client,
    request: &CreateIntegrationRequest,
    idempotency_key: Option<&str>,
) -> Result<Integration> {
    client.post_data_with_headers(
        path::INTEGRATIONS,
        request,
        &idempotency_headers(idempotency_key),
    )
}

/// Change an integration. New credentials put `status` back to `pending`
/// until the new connection is up.
pub fn update_integration(
    client: &Client,
    id: &str,
    request: &UpdateIntegrationRequest,
    idempotency_key: Option<&str>,
) -> Result<Integration> {
    client.patch_data_with_headers(
        &path::integration(id),
        request,
        &idempotency_headers(idempotency_key),
    )
}

/// Disconnect the bot and delete the integration. Conversation history is
/// kept.
pub fn delete_integration(client: &Client, id: &str, idempotency_key: Option<&str>) -> Result<()> {
    client
        .delete_data_with_headers::<EmptyData>(
            &path::integration(id),
            &idempotency_headers(idempotency_key),
        )
        .map(|_| ())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn credentials_parse_as_an_object_of_strings_and_nothing_else() {
        let creds =
            IntegrationCredentials::from_json(r#"{"botToken":"xoxb-1","appToken":"xapp-1"}"#)
                .unwrap();
        assert_eq!(
            serde_json::to_string(&creds).unwrap(),
            r#"{"appToken":"xapp-1","botToken":"xoxb-1"}"#
        );
        for (json, needle) in [
            ("[]", "JSON object"),
            ("{}", "empty"),
            (r#"{"appSecret":""}"#, "is empty"),
            (r#"{"appSecret":1}"#, "must be a string"),
            ("nope", "not valid JSON"),
        ] {
            let err = IntegrationCredentials::from_json(json).unwrap_err();
            assert!(err.contains(needle), "{json}: {err}");
        }
    }

    #[test]
    fn a_channel_takes_exactly_its_two_fields() {
        let slack =
            IntegrationCredentials::from_json(r#"{"botToken":"b","appToken":"a"}"#).unwrap();
        slack.check_complete_for("slack").unwrap();
        let err = slack.check_complete_for("dingtalk").unwrap_err();
        assert!(err.contains("appKey and appSecret"), "{err}");

        let partial = IntegrationCredentials::from_json(r#"{"appSecret":"s"}"#).unwrap();
        assert!(partial.check_complete_for("feishu").is_err());
        partial.check_known().unwrap();
        let bogus = IntegrationCredentials::from_json(r#"{"token":"s"}"#).unwrap();
        assert!(bogus.check_known().unwrap_err().contains("`token`"));
    }

    #[test]
    fn debug_output_never_shows_a_value() {
        let creds = IntegrationCredentials::from_json(r#"{"secret":"hunter2"}"#).unwrap();
        let debug = format!("{creds:?}");
        assert!(
            debug.contains("secret") && !debug.contains("hunter2"),
            "{debug}"
        );
    }
}
