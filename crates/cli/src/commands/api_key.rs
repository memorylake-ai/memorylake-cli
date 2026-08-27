//! `memorylake api-key` / `key` commands.

use anyhow::{Context, Result};
use clap::Subcommand;
use memorylake_core::api::admin::{
    CreateApiKeyRequest, ListParams, create_api_key, get_api_key, list_api_keys, revoke_api_key,
    rotate_api_key,
};

use super::{api_client, print_json};

/// API key subcommands.
#[derive(Debug, Subcommand)]
pub enum ApiKeyCommand {
    /// List the team's API keys. The keys themselves are never returned —
    /// only their prefixes.
    List {
        /// Number of items per page.
        #[arg(long)]
        page_size: Option<u32>,
        /// Continuation token from a previous response.
        #[arg(long)]
        continuation_token: Option<String>,
        /// Fuzzy filter by key name (partial match).
        #[arg(long = "name")]
        name_fuzzy: Option<String>,
    },
    /// Get a single API key by id.
    Get {
        /// API key id.
        id: String,
    },
    /// Create an API key. The full key is printed exactly once — save it.
    Create {
        /// Display name for the key.
        #[arg(long)]
        name: String,
        /// Issue the key for this virtual member (see `member create`); the
        /// key then acts as that member. Human members cannot be targeted.
        #[arg(long = "member", value_name = "PRINCIPAL_ID")]
        member_principal_id: Option<String>,
        /// Expiry, Unix seconds. Omit for a key that never expires.
        #[arg(long)]
        expires_at: Option<i64>,
        /// Retrying with the same value replays the first result instead of
        /// creating a second key.
        #[arg(long)]
        idempotency_key: Option<String>,
    },
    /// Replace a key's material and print the new value once. The previous
    /// value stops working immediately.
    Rotate {
        /// API key id.
        id: String,
        /// Retrying with the same value replays the first result instead of
        /// minting a second key.
        #[arg(long)]
        idempotency_key: Option<String>,
    },
    /// Delete an API key. The key making the request cannot revoke itself.
    Revoke {
        /// API key id.
        id: String,
        /// Retrying with the same value replays the first result.
        #[arg(long)]
        idempotency_key: Option<String>,
    },
}

/// Execute an `api-key` subcommand.
pub fn run(
    command: ApiKeyCommand,
    profile: Option<String>,
    base_url: Option<String>,
) -> Result<()> {
    let client = api_client(profile, base_url)?;

    match command {
        ApiKeyCommand::List {
            page_size,
            continuation_token,
            name_fuzzy,
        } => {
            let data = list_api_keys(
                &client,
                &ListParams {
                    page_size,
                    continuation_token,
                    name_fuzzy,
                },
            )
            .context("list API keys")?;
            print_json(&data)
        }
        ApiKeyCommand::Get { id } => {
            let data = get_api_key(&client, &id).context("get API key")?;
            print_json(&data)
        }
        ApiKeyCommand::Create {
            name,
            member_principal_id,
            expires_at,
            idempotency_key,
        } => {
            let data = create_api_key(
                &client,
                &CreateApiKeyRequest {
                    name,
                    member_principal_id,
                    expires_at,
                },
                idempotency_key.as_deref(),
            )
            .context("create API key")?;
            print_json(&data)
        }
        ApiKeyCommand::Rotate {
            id,
            idempotency_key,
        } => {
            let data = rotate_api_key(&client, &id, idempotency_key.as_deref())
                .context("rotate API key")?;
            print_json(&data)
        }
        ApiKeyCommand::Revoke {
            id,
            idempotency_key,
        } => {
            revoke_api_key(&client, &id, idempotency_key.as_deref()).context("revoke API key")?;
            println!("API key {id} revoked");
            Ok(())
        }
    }
}
