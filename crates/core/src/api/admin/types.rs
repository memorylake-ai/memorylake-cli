//! Shared team-management resource types.
//!
//! Response structs capture the documented fields and collect anything else
//! the server sends into `extra` via `#[serde(flatten)]`: these endpoints keep
//! gaining fields, and a decoder that silently dropped them once hid the
//! key owner, IP restriction and charged tokens from every listing.

use serde::{Deserialize, Serialize};
use serde_json::{Map, Value};

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

/// Append `name=value` when a value is given.
pub(super) fn push_opt<T: ToString>(
    query: &mut Vec<(&'static str, String)>,
    name: &'static str,
    value: Option<&T>,
) {
    if let Some(value) = value {
        query.push((name, value.to_string()));
    }
}

/// Append `name=value` once per value: the management API's repeatable
/// filters (`email=a&email=b`) take several values this way, not as a
/// comma-separated list.
pub(super) fn push_each(
    query: &mut Vec<(&'static str, String)>,
    name: &'static str,
    values: &[String],
) {
    for value in values {
        query.push((name, value.clone()));
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
    /// Fields the CLI does not model yet, kept so printing this never drops
    /// what the server sent.
    #[serde(flatten)]
    pub extra: Map<String, Value>,
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
    /// Last 4 characters of the key, to match a key someone holds. Absent for
    /// keys whose plaintext is not retained.
    #[serde(default)]
    pub key_suffix: Option<String>,
    /// `enabled`, `disabled`, or `expired`.
    pub status: String,
    /// The member this key acts as: its requests carry that member's
    /// permissions and usage is attributed to them.
    #[serde(default)]
    pub owner: Option<ApiKeyOwner>,
    /// Client IPs or CIDR ranges the key may be used from. Empty means no
    /// restriction.
    #[serde(default)]
    pub allowed_ips: Vec<String>,
    /// Model-routing group. Absent means the owner's own group.
    #[serde(default)]
    pub group: Option<String>,
    /// Fallback model-routing group. Absent means none.
    #[serde(default)]
    pub backup_group: Option<String>,
    /// true for keys the platform created for its own use (the console
    /// playground, for example).
    #[serde(default)]
    pub is_system: bool,
    /// Creation time, Unix seconds.
    pub created_at: i64,
    /// Expiry time, Unix seconds. Absent when the key never expires.
    #[serde(default)]
    pub expires_at: Option<i64>,
    /// Last-used time, Unix seconds. Approximate — the server throttles
    /// updates, so it can lag real usage by up to an hour.
    #[serde(default)]
    pub last_used_at: Option<i64>,
    /// Fields the CLI does not model yet, kept so printing this never drops
    /// what the server sent.
    #[serde(flatten)]
    pub extra: Map<String, Value>,
}

/// The member an API key acts as.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ApiKeyOwner {
    /// The member's `principal_id`.
    pub principal_id: String,
    /// The member's display name.
    pub display_name: String,
    /// `human` or `virtual`, as on [`Member::member_type`].
    pub member_type: String,
    /// Fields the CLI does not model yet, kept so printing this never drops
    /// what the server sent.
    #[serde(flatten)]
    pub extra: Map<String, Value>,
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
    /// Last 4 characters of the key.
    #[serde(default)]
    pub key_suffix: Option<String>,
    /// The full API key, including the `sk-` prefix. An idempotent replay
    /// omits it — the secret is only ever sent on the live first response.
    #[serde(default)]
    pub key: Option<String>,
    /// Fields the CLI does not model yet, kept so printing this never drops
    /// what the server sent.
    #[serde(flatten)]
    pub extra: Map<String, Value>,
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
    /// Fields the CLI does not model yet, kept so printing this never drops
    /// what the server sent.
    #[serde(flatten)]
    pub extra: Map<String, Value>,
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
    /// Fields the CLI does not model yet, kept so printing this never drops
    /// what the server sent.
    #[serde(flatten)]
    pub extra: Map<String, Value>,
}

/// Quota snapshot plus consumption over a period.
///
/// `quota` is an account-level snapshot taken now; `totals` and `by_model`
/// aggregate over the requested period. They answer different questions.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Usage {
    /// Whose consumption the figures cover: `team` (the caller may view the
    /// whole team's usage) or `self` (only the caller's own, API keys
    /// included).
    pub scope: String,
    /// The period the totals cover.
    pub period: UsagePeriod,
    /// Account-level quota snapshot — not scoped to the period.
    pub quota: UsageQuota,
    /// Aggregate consumption over the period.
    pub totals: UsageTotals,
    /// Per-model breakdown over the period.
    #[serde(default)]
    pub by_model: Vec<UsageByModel>,
    /// Per-member breakdown, largest consumer first. Only when grouped by
    /// member.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub by_member: Option<Vec<UsageByMember>>,
    /// One entry per day, oldest first, idle days included as zeros. Only
    /// when a daily series was asked for.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub buckets: Option<Vec<UsageBucket>>,
    /// Fields the CLI does not model yet, kept so printing this never drops
    /// what the server sent.
    #[serde(flatten)]
    pub extra: Map<String, Value>,
}

/// First and last day a usage report covers.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct UsagePeriod {
    /// First day included, `YYYY-MM-DD`.
    pub start_date: String,
    /// Last day included, `YYYY-MM-DD`.
    pub end_date: String,
    /// IANA timezone the days are cut in.
    pub timezone: String,
    /// Fields the CLI does not model yet, kept so printing this never drops
    /// what the server sent.
    #[serde(flatten)]
    pub extra: Map<String, Value>,
}

/// Account-level quota snapshot.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct UsageQuota {
    /// Currently available tokens. Not meaningful when `unlimited` is true.
    pub available_tokens: i64,
    /// When true, `available_tokens` is not meaningful.
    pub unlimited: bool,
    /// Fields the CLI does not model yet, kept so printing this never drops
    /// what the server sent.
    #[serde(flatten)]
    pub extra: Map<String, Value>,
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
    /// Consumption charged against the balance, in tokens — the unit of
    /// `quota.available_tokens`. Covers platform operations too, which carry
    /// no prompt or completion tokens.
    pub used_tokens: i64,
    /// Fields the CLI does not model yet, kept so printing this never drops
    /// what the server sent.
    #[serde(flatten)]
    pub extra: Map<String, Value>,
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
    /// Consumption charged against the balance, in tokens.
    pub used_tokens: i64,
    /// Fields the CLI does not model yet, kept so printing this never drops
    /// what the server sent.
    #[serde(flatten)]
    pub extra: Map<String, Value>,
}

/// One member's consumption over a period.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct UsageByMember {
    /// The member's `principal_id`. Includes people who have since left.
    pub principal_id: String,
    /// The member's display name; may be empty for callers who are not
    /// owners or admins.
    pub display_name: String,
    /// Number of requests.
    pub requests: i64,
    /// Prompt tokens consumed.
    pub prompt_tokens: i64,
    /// Completion tokens produced.
    pub completion_tokens: i64,
    /// Consumption charged against the balance, in tokens.
    pub used_tokens: i64,
    /// Fields the CLI does not model yet, kept so printing this never drops
    /// what the server sent.
    #[serde(flatten)]
    pub extra: Map<String, Value>,
}

/// One day of a usage report's daily series.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct UsageBucket {
    /// The day, `YYYY-MM-DD`, in the period's timezone.
    pub date: String,
    /// Bucket start, Unix seconds (inclusive).
    pub start_time: i64,
    /// Bucket end, Unix seconds (exclusive).
    pub end_time: i64,
    /// Number of requests.
    pub requests: i64,
    /// Prompt tokens consumed.
    pub prompt_tokens: i64,
    /// Completion tokens produced.
    pub completion_tokens: i64,
    /// Consumption charged against the balance, in tokens.
    pub used_tokens: i64,
    /// This day's per-model breakdown, when grouped by model.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub by_model: Option<Vec<UsageByModel>>,
    /// This day's per-member breakdown, when grouped by member.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub by_member: Option<Vec<UsageByMember>>,
    /// Fields the CLI does not model yet, kept so printing this never drops
    /// what the server sent.
    #[serde(flatten)]
    pub extra: Map<String, Value>,
}

/// Consumption today, over the trailing 7 days, and month to date.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct UsageSummary {
    /// `team` or `self`, as on [`Usage::scope`].
    pub scope: String,
    /// IANA timezone the days are cut in.
    pub timezone: String,
    /// Today.
    pub today: UsageSummaryWindow,
    /// The trailing 7 days, today included.
    pub last_7_days: UsageSummaryWindow,
    /// From the first of the current month through today.
    pub month_to_date: UsageSummaryWindow,
    /// Fields the CLI does not model yet, kept so printing this never drops
    /// what the server sent.
    #[serde(flatten)]
    pub extra: Map<String, Value>,
}

/// One window of a [`UsageSummary`].
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct UsageSummaryWindow {
    /// First day included, `YYYY-MM-DD`.
    pub start_date: String,
    /// Last day included, `YYYY-MM-DD`.
    pub end_date: String,
    /// Number of requests.
    pub requests: i64,
    /// Prompt tokens consumed.
    pub prompt_tokens: i64,
    /// Completion tokens produced.
    pub completion_tokens: i64,
    /// Consumption charged against the balance, in tokens.
    pub used_tokens: i64,
    /// Fields the CLI does not model yet, kept so printing this never drops
    /// what the server sent.
    #[serde(flatten)]
    pub extra: Map<String, Value>,
}

/// One billed model request or platform operation.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct UsageRecord {
    /// Record id.
    pub id: String,
    /// When the consumption was recorded, Unix seconds.
    pub created_at: i64,
    /// `model_call` or `platform_task`.
    pub kind: String,
    /// Model name, or the platform operation.
    pub model: String,
    /// The member the consumption is attributed to, when the record names
    /// one.
    #[serde(default)]
    pub member: Option<UsageRecordMember>,
    /// Prompt (input) tokens.
    pub prompt_tokens: i64,
    /// Completion (output) tokens.
    pub completion_tokens: i64,
    /// Consumption charged against the balance, in tokens.
    pub used_tokens: i64,
    /// Request id, when one was recorded.
    #[serde(default)]
    pub request_id: Option<String>,
    /// Trace id, when one was recorded.
    #[serde(default)]
    pub trace_id: Option<String>,
    /// Fields the CLI does not model yet, kept so printing this never drops
    /// what the server sent.
    #[serde(flatten)]
    pub extra: Map<String, Value>,
}

/// The member a [`UsageRecord`] is attributed to.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct UsageRecordMember {
    /// The member's `principal_id`.
    pub principal_id: String,
    /// The member's display name.
    #[serde(default)]
    pub display_name: Option<String>,
    /// Fields the CLI does not model yet, kept so printing this never drops
    /// what the server sent.
    #[serde(flatten)]
    pub extra: Map<String, Value>,
}

/// A team just created, with its owner key when one was asked for.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CreatedTeam {
    /// The new team.
    pub team: Team,
    /// Owner API key for the new team, shown exactly once. Present only when
    /// one was asked for; an idempotent replay omits its secret.
    #[serde(default)]
    pub api_key: Option<ApiKeyCreated>,
    /// Fields the CLI does not model yet, kept so printing this never drops
    /// what the server sent.
    #[serde(flatten)]
    pub extra: Map<String, Value>,
}

/// Outcome of deleting the team.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct DeletedTeam {
    /// The deleted team's id.
    pub tenant_id: String,
    /// Deletion time, Unix seconds.
    pub dissolved_at: i64,
    /// true when a paid subscription was cancelled first.
    pub subscription_canceled: bool,
    /// Fields the CLI does not model yet, kept so printing this never drops
    /// what the server sent.
    #[serde(flatten)]
    pub extra: Map<String, Value>,
}

/// Outcome of handing the team to a new owner.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct OwnershipTransfer {
    /// The team's id.
    pub tenant_id: String,
    /// The owner before the transfer, now an admin.
    pub previous_owner_principal_id: String,
    /// The new owner.
    pub new_owner_principal_id: String,
    /// Fields the CLI does not model yet, kept so printing this never drops
    /// what the server sent.
    #[serde(flatten)]
    pub extra: Map<String, Value>,
}
