//! Shared team-management resource types.

use serde::{Deserialize, Serialize};

/// Cursor-paged list envelope shared by every management list endpoint.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Page<T> {
    /// Items on this page.
    #[serde(default = "Vec::new")]
    pub items: Vec<T>,
    /// Total items matching the filter across all pages, when the server
    /// reports one.
    #[serde(default)]
    pub total: Option<i64>,
    /// Token for the next page; absent on the last page. Opaque — do not
    /// construct or parse it.
    #[serde(default)]
    pub continuation_token: Option<String>,
}

/// Query parameters shared by the API-key and member list endpoints.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct ListParams {
    /// Page size (server default 20, maximum 100).
    pub page_size: Option<u32>,
    /// Continuation token from a previous page.
    pub continuation_token: Option<String>,
    /// Case-insensitive substring filter on the name. Sent as `name_fuzzy`.
    pub name_fuzzy: Option<String>,
}

impl ListParams {
    /// Render these parameters as a query string, `name_fuzzy` included.
    pub(super) fn to_query(&self) -> Vec<(&'static str, String)> {
        let mut query = Vec::new();
        push_page_query(
            &mut query,
            self.page_size,
            self.continuation_token.as_deref(),
        );
        if let Some(name_fuzzy) = &self.name_fuzzy {
            query.push(("name_fuzzy", name_fuzzy.clone()));
        }
        query
    }
}

/// Append the pagination parameters every list endpoint shares.
pub(super) fn push_page_query(
    query: &mut Vec<(&'static str, String)>,
    page_size: Option<u32>,
    continuation_token: Option<&str>,
) {
    if let Some(page_size) = page_size {
        query.push(("page_size", page_size.to_string()));
    }
    if let Some(token) = continuation_token {
        query.push(("continuation_token", token.to_string()));
    }
}

/// The team the calling key belongs to.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Team {
    /// Stable team id.
    pub id: String,
    /// Display name.
    pub name: String,
    /// `personal` (single-user space) or `org` (team with members).
    #[serde(rename = "type")]
    pub team_type: String,
    /// `principal_id` of the team owner (joins to [`Member::principal_id`]).
    pub owner_principal_id: String,
    /// Creation time, Unix seconds.
    pub created_at: i64,
    /// The calling key's role in this team.
    #[serde(default)]
    pub caller_role: Option<String>,
}

/// An API key of the team. Never carries the key itself — only
/// [`ApiKeyCreated`] does, once.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ApiKey {
    /// Stable key id.
    pub id: String,
    /// Display name.
    pub name: String,
    /// First 8 characters of the key body.
    pub key_prefix: String,
    /// `enabled`, `disabled`, or `expired`.
    pub status: String,
    /// Creation time, Unix seconds.
    pub created_at: i64,
    /// Expiry time, Unix seconds. Absent when the key never expires.
    #[serde(default)]
    pub expires_at: Option<i64>,
    /// Last-used time, Unix seconds. Approximate — the server throttles
    /// updates, so it can lag real usage by up to an hour.
    #[serde(default)]
    pub last_used_at: Option<i64>,
}

/// The one response shape that carries a full API key. Shown exactly once by
/// create and rotate; read endpoints never return it.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ApiKeyCreated {
    /// Stable key id.
    pub id: String,
    /// Display name.
    pub name: String,
    /// First 8 characters of the key body.
    pub key_prefix: String,
    /// The full API key, including the `sk-` prefix. An idempotent replay
    /// omits it — the secret is only ever sent on the live first response.
    #[serde(default)]
    pub key: Option<String>,
}

/// A team member.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Member {
    /// Stable member id.
    pub principal_id: String,
    /// Display name.
    pub display_name: String,
    /// `human` (joined via invitation) or `virtual` (managed identity that
    /// acts only through API keys issued for it).
    pub member_type: String,
    /// Role in this team: `tenant_owner`, `tenant_admin`, `tenant_member`, or
    /// a custom role key.
    pub role: String,
    /// Only returned to team owners and admins.
    #[serde(default)]
    pub email: Option<String>,
    /// Only returned to team owners and admins.
    #[serde(default)]
    pub username: Option<String>,
    /// Join time, Unix seconds.
    pub joined_at: i64,
    /// `active` or `inactive`.
    pub status: String,
    /// Tokens consumed by this member.
    pub used_tokens: i64,
    /// Per-member cap in tokens. Absent when uncapped.
    #[serde(default)]
    pub max_tokens: Option<i64>,
}

/// A team invitation. Never carries the invite token — that token is the
/// accept credential.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Invitation {
    /// Stable invitation id.
    pub id: String,
    /// Invitee email (normalised to lowercase).
    pub email: String,
    /// Role the invitee will hold on acceptance.
    pub role: String,
    /// `pending`, `accepted`, `rejected`, `expired`, or `revoked`.
    pub status: String,
    /// First invitation time, Unix seconds. Re-inviting does not reset it.
    pub created_at: i64,
    /// Expiry of the current invitation email, Unix seconds.
    pub expires_at: i64,
    /// Most recent invitation email time, Unix seconds.
    #[serde(default)]
    pub last_invited_at: Option<i64>,
    /// Acceptance time, Unix seconds. Present only when accepted.
    #[serde(default)]
    pub accepted_at: Option<i64>,
    /// `principal_id` of the member this invitation produced. Present only
    /// when accepted.
    #[serde(default)]
    pub accepted_principal_id: Option<String>,
}

/// Quota snapshot plus consumption over a period.
///
/// `quota` is an account-level snapshot taken now; `totals` and `by_model`
/// aggregate over the requested period. They answer different questions.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Usage {
    /// The period the totals cover.
    pub period: UsagePeriod,
    /// Account-level quota snapshot — not scoped to the period.
    pub quota: UsageQuota,
    /// Aggregate consumption over the period.
    pub totals: UsageTotals,
    /// Per-model breakdown over the period.
    #[serde(default)]
    pub by_model: Vec<UsageByModel>,
}

/// First and last day a usage report covers.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct UsagePeriod {
    /// First day included, `YYYY-MM-DD`.
    pub start_date: String,
    /// Last day included, `YYYY-MM-DD`.
    pub end_date: String,
}

/// Account-level quota snapshot.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct UsageQuota {
    /// Currently available tokens. Not meaningful when `unlimited` is true.
    pub available_tokens: i64,
    /// When true, `available_tokens` is not meaningful.
    pub unlimited: bool,
}

/// Aggregate consumption over a period.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct UsageTotals {
    /// Number of requests.
    pub requests: i64,
    /// Prompt tokens consumed.
    pub prompt_tokens: i64,
    /// Completion tokens produced.
    pub completion_tokens: i64,
}

/// One model's (or platform metering operation's) consumption over a period.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct UsageByModel {
    /// Model name, or a platform metering operation (`files_process`,
    /// `memory_input`, `retrieval_call`, `search_call`).
    pub model: String,
    /// Number of requests.
    pub requests: i64,
    /// Prompt tokens consumed.
    pub prompt_tokens: i64,
    /// Completion tokens produced.
    pub completion_tokens: i64,
}
