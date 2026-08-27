//! API keys of the team (`/admin/v1/api-keys`).

use serde::Serialize;

use crate::client::Client;
use crate::error::Result;

use super::path;
use super::types::{ApiKey, ApiKeyCreated, ListParams, Page};
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
}

/// List the team's API keys. Callers holding only own-scope read permission
/// see just the keys they created.
pub fn list_api_keys(client: &Client, params: &ListParams) -> Result<Page<ApiKey>> {
    client.get_data(path::API_KEYS, &params.to_query())
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
