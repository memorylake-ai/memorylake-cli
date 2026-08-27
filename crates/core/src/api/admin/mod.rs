//! Team management API (`/admin/v1/*`).
//!
//! Governs the team the API key belongs to: the team itself, its API keys,
//! members, invitations, and usage. Same base URL and same key as every other
//! module here — the management endpoints simply live under `/admin/v1`
//! instead of `/api/v1`. The team is fixed by the key; nothing in a request
//! can address a different one.
//!
//! Write endpoints accept an optional `Idempotency-Key` header: retrying with
//! the same value replays the first result instead of repeating the write.
//! That matters most for [`api_keys::create_api_key`] and
//! [`api_keys::rotate_api_key`], whose responses carry a secret shown exactly
//! once.

mod api_keys;
mod invitations;
mod members;
mod path;
mod roles;
mod team;
mod types;
mod usage;

pub use api_keys::{
    CreateApiKeyRequest, create_api_key, get_api_key, list_api_keys, revoke_api_key, rotate_api_key,
};
pub use invitations::{
    CreateInvitationRequest, ListInvitationsParams, create_invitation, list_invitations,
    revoke_invitation,
};
pub use members::{
    CreateMemberRequest, create_member, list_members, remove_member, set_member_role,
};
pub use roles::{Role, RoleList, list_roles};
pub use team::{get_team, rename_team};
pub use types::{
    ApiKey, ApiKeyCreated, Invitation, ListParams, Member, Page, Team, Usage, UsageByModel,
    UsagePeriod, UsageQuota, UsageTotals,
};
pub use usage::{GetUsageParams, get_usage};

/// Header that makes a retried write replay its first result.
const IDEMPOTENCY_KEY_HEADER: &str = "Idempotency-Key";

/// Render an optional idempotency key as the extra-headers slice the client
/// takes. `None` means "send nothing", not an empty header value.
fn idempotency_headers(key: Option<&str>) -> Vec<(&'static str, &str)> {
    key.map(|key| vec![(IDEMPOTENCY_KEY_HEADER, key)])
        .unwrap_or_default()
}

/// Discardable payload of writes that answer `{"success":true,"data":{}}`.
///
/// `serde_json::Value` rather than `()` or an empty struct: `{}` cannot
/// deserialize into `()`, and an empty struct would reject the equally valid
/// absent-`data` form (which decodes as `Value::Null`).
type EmptyData = serde_json::Value;
