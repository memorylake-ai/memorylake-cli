//! API keys of the team (`/admin/v1/api-keys`).

use serde::Serialize;

use crate::client::Client;
use crate::error::Result;

use super::path;
use super::types::{ApiKey, ApiKeyCreated, Page, push_opt, push_page_query};
use super::{EmptyData, idempotency_headers};

/// Request body for creating an API key.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize)]
pub struct CreateApiKeyRequest {
    /// Display name for the key.
    pub name: String,
    /// Issue the key for this VIRTUAL member (from `member create`); the key
    /// then acts as that member. Human members cannot be targeted.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub member_principal_id: Option<String>,
    /// Expiry, Unix seconds. Omit for a key that never expires.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub expires_at: Option<i64>,
    /// Client IPs or CIDR ranges the key may be used from. Omit for no
    /// restriction.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub allowed_ips: Option<Vec<String>>,
    /// Model-routing group: a public group or the owner's own.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub group: Option<String>,
    /// Fallback model-routing group, same rule as `group`.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub backup_group: Option<String>,
}

/// Request body for changing an API key. Only the fields set are sent, and
/// only those change; the server rejects `null`.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize)]
pub struct UpdateApiKeyRequest {
    /// New display name. Usage records keep the name at the time of each
    /// request, so renaming splits the key's history.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,
    /// `enabled` or `disabled`. An expired key can be enabled only together
    /// with a future `expires_at`.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub status: Option<String>,
    /// New expiry, Unix seconds; `0` means never.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub expires_at: Option<i64>,
    /// Replacement IP / CIDR restriction; an empty list removes it.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub allowed_ips: Option<Vec<String>>,
    /// Model-routing group; `""` clears it.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub group: Option<String>,
    /// Fallback model-routing group; `""` clears it.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub backup_group: Option<String>,
}

/// Query parameters for listing API keys.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct ListApiKeysParams {
    /// Page size (server default 20, maximum 100).
    pub page_size: Option<u32>,
    /// Continuation token from a previous page.
    pub continuation_token: Option<String>,
    /// Case-insensitive substring filter on the key name.
    pub name_fuzzy: Option<String>,
    /// Only keys in this state: `enabled`, `disabled`, or `expired`.
    pub status: Option<String>,
    /// Only keys owned by this member. A caller allowed to read only their
    /// own keys is refused (403) for anyone else.
    pub owner_principal_id: Option<String>,
}

/// List the team's API keys. Callers holding only own-scope read permission
/// see just the keys they created.
pub fn list_api_keys(client: &Client, params: &ListApiKeysParams) -> Result<Page<ApiKey>> {
    let mut query = Vec::new();
    push_page_query(
        &mut query,
        params.page_size,
        params.continuation_token.as_deref(),
    );
    push_opt(&mut query, "name_fuzzy", params.name_fuzzy.as_ref());
    push_opt(&mut query, "status", params.status.as_ref());
    push_opt(
        &mut query,
        "owner_principal_id",
        params.owner_principal_id.as_ref(),
    );
    client.get_data(path::API_KEYS, &query)
}

/// Change an API key's name, status, expiry, IP restriction, or routing
/// groups. The key making the request cannot disable itself.
///
/// The endpoint documents no `Idempotency-Key`: the patch is idempotent by
/// itself.
pub fn update_api_key(client: &Client, id: &str, request: &UpdateApiKeyRequest) -> Result<ApiKey> {
    client.patch_data(&path::api_key(id), request)
}

/// Return one API key. Keys of other teams are reported as not found.
pub fn get_api_key(client: &Client, id: &str) -> Result<ApiKey> {
    client.get_data(&path::api_key(id), &[])
}

/// Create an API key. The response carries the full key exactly once.
pub fn create_api_key(
    client: &Client,
    request: &CreateApiKeyRequest,
    idempotency_key: Option<&str>,
) -> Result<ApiKeyCreated> {
    client.post_data_with_headers(
        path::API_KEYS,
        request,
        &idempotency_headers(idempotency_key),
    )
}

/// Replace the key material and return the new value once. The previous value
/// stops working immediately.
pub fn rotate_api_key(
    client: &Client,
    id: &str,
    idempotency_key: Option<&str>,
) -> Result<ApiKeyCreated> {
    // The endpoint takes no body; an empty object keeps the request valid
    // JSON under the client's `application/json` default.
    client.post_data_with_headers(
        &path::api_key_rotate(id),
        &serde_json::json!({}),
        &idempotency_headers(idempotency_key),
    )
}

/// Delete an API key. The key used to make the request cannot revoke itself.
pub fn revoke_api_key(client: &Client, id: &str, idempotency_key: Option<&str>) -> Result<()> {
    client
        .delete_data_with_headers::<EmptyData>(
            &path::api_key(id),
            &idempotency_headers(idempotency_key),
        )
        .map(|_| ())
}
