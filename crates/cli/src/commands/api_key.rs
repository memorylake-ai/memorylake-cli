//! `memorylake api-key` / `key` commands.

use anyhow::{Context, Result, bail};
use clap::Subcommand;
use memorylake_core::api::admin::{
    CreateApiKeyRequest, ListApiKeysParams, UpdateApiKeyRequest, create_api_key, get_api_key,
    list_api_keys, revoke_api_key, rotate_api_key, update_api_key,
};

use super::{api_client, print_json};

/// API key subcommands.
#[derive(Debug, Subcommand)]
pub enum ApiKeyCommand {
    /// List the team's API keys. The keys themselves are never returned —
    /// only their prefixes and suffixes.
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
        /// Only keys in this state.
        #[arg(long, value_parser = ["enabled", "disabled", "expired"])]
        status: Option<String>,
        /// Only keys owned by this member (principal id, see `member list`).
        #[arg(long = "owner", value_name = "PRINCIPAL_ID")]
        owner_principal_id: Option<String>,
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
        /// Only allow requests from this IP or CIDR range. Repeat, or
        /// separate with commas, for several. Omit for no restriction.
        #[arg(long = "allowed-ip", value_name = "IP_OR_CIDR", value_delimiter = ',')]
        allowed_ips: Vec<String>,
        /// Model-routing group: a public group or the owner's own.
        #[arg(long)]
        group: Option<String>,
        /// Fallback model-routing group, same rule as --group.
        #[arg(long)]
        backup_group: Option<String>,
        /// Retrying with the same value replays the first result instead of
        /// creating a second key.
        #[arg(long)]
        idempotency_key: Option<String>,
    },
    /// Change a key's name, status, expiry, IP restriction, or routing
    /// groups. Only the flags given change. The key making the request
    /// cannot disable itself.
    Update {
        /// API key id.
        id: String,
        /// New display name. Usage records keep the name each request was
        /// made under, so renaming splits the key's history.
        #[arg(long)]
        name: Option<String>,
        /// Enable or disable the key. An expired key can be enabled only
        /// together with a future --expires-at.
        #[arg(long, value_parser = ["enabled", "disabled"])]
        status: Option<String>,
        /// New expiry, Unix seconds; 0 means the key never expires.
        #[arg(long, value_parser = clap::value_parser!(i64).range(0..))]
        expires_at: Option<i64>,
        /// Replace the IP restriction with these IPs or CIDR ranges. Repeat,
        /// or separate with commas, for several.
        #[arg(
            long = "allowed-ip",
            value_name = "IP_OR_CIDR",
            value_delimiter = ',',
            conflicts_with = "no_ip_restriction"
        )]
        allowed_ips: Vec<String>,
        /// Remove the IP restriction.
        #[arg(long)]
        no_ip_restriction: bool,
        /// Model-routing group; "" clears it.
        #[arg(long)]
        group: Option<String>,
        /// Fallback model-routing group; "" clears it.
        #[arg(long)]
        backup_group: Option<String>,
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
    // An empty update is a caller mistake; say so before asking for
    // credentials.
    if let ApiKeyCommand::Update {
        name: None,
        status: None,
        expires_at: None,
        allowed_ips,
        no_ip_restriction: false,
        group: None,
        backup_group: None,
        ..
    } = &command
        && allowed_ips.is_empty()
    {
        bail!(
            "nothing to update; pass --name, --status, --expires-at, --allowed-ip, \
             --no-ip-restriction, --group or --backup-group"
        );
    }

    let client = api_client(profile, base_url)?;

    match command {
        ApiKeyCommand::List {
            page_size,
            continuation_token,
            name_fuzzy,
            status,
            owner_principal_id,
        } => {
            let data = list_api_keys(
                &client,
                &ListApiKeysParams {
                    page_size,
                    continuation_token,
                    name_fuzzy,
                    status,
                    owner_principal_id,
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
            allowed_ips,
            group,
            backup_group,
            idempotency_key,
        } => {
            let data = create_api_key(
                &client,
                &CreateApiKeyRequest {
                    name,
                    member_principal_id,
                    expires_at,
                    allowed_ips: (!allowed_ips.is_empty()).then_some(allowed_ips),
                    group,
                    backup_group,
                },
                idempotency_key.as_deref(),
            )
            .context("create API key")?;
            print_json(&data)
        }
        ApiKeyCommand::Update {
            id,
            name,
            status,
            expires_at,
            allowed_ips,
            no_ip_restriction,
            group,
            backup_group,
        } => {
            let allowed_ips = if no_ip_restriction {
                Some(Vec::new())
            } else {
                (!allowed_ips.is_empty()).then_some(allowed_ips)
            };
            let data = update_api_key(
                &client,
                &id,
                &UpdateApiKeyRequest {
                    name,
                    status,
                    expires_at,
                    allowed_ips,
                    group,
                    backup_group,
                },
            )
            .context("update API key")?;
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
