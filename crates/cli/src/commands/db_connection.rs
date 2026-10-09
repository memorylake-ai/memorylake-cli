//! `memorylake db-connection` / `dbconn` commands.
//!
//! A connection records how to reach a database and the credentials for it.
//! It is the first link in the chain connection → datasource (`datasource`)
//! → project database memory (`project database`).
//!
//! Passwords never travel on the command line; see [`password`].

mod password;

use anyhow::{Context, Result, anyhow, bail};
use clap::Subcommand;
use memorylake_core::Error as CoreError;
use memorylake_core::api::db_connections::{
    ConnectivityTestRequest, CreateDbConnectionRequest, DbPassword, ListDbConnectionsParams,
    UpdateDbConnectionRequest, create_db_connection, delete_db_connection, get_db_connection,
    get_db_connection_by_custom_id, list_db_connection_schemas, list_db_connections,
    test_db_connectivity, update_db_connection,
};

use super::input::{parse_custom_id, parse_non_empty};
use super::{api_client, print_json};
use password::{PasswordArgs, PasswordNeed};

/// Database connection subcommands.
///
/// Connections are not scoped to a workspace, so none of these take
/// `--workspace`.
#[derive(Debug, Subcommand)]
pub enum DbConnectionCommand {
    /// List the database connections you can open.
    List {
        /// Only connections whose name contains this text (case-insensitive).
        #[arg(long = "name")]
        name_fuzzy: Option<String>,
        /// Number of items per page (1-100).
        #[arg(long, value_parser = clap::value_parser!(u32).range(1..=100))]
        page_size: Option<u32>,
        /// Continuation token from a previous response.
        #[arg(long)]
        continuation_token: Option<String>,
    },
    /// Save a connection to a database.
    ///
    /// Run `db-connection test` first with the same host, port, username,
    /// database, and password source: a failed `create` may only say
    /// INTERNAL_ERROR, while `test` names credential and reachability
    /// problems (DB_CONNECTION_FAILED).
    Create {
        /// Display name.
        #[arg(long, value_parser = parse_non_empty)]
        name: String,
        /// Server host name or address.
        #[arg(long, value_parser = parse_non_empty)]
        host: String,
        /// Server port. The server defaults to 5432.
        #[arg(long, value_parser = clap::value_parser!(u16).range(1..))]
        port: Option<u16>,
        /// User name to authenticate with.
        #[arg(long, value_parser = parse_non_empty)]
        username: String,
        /// Name of the database to read.
        #[arg(long, value_parser = parse_non_empty)]
        database: String,
        /// Free-form description.
        #[arg(long)]
        description: Option<String>,
        /// Caller-defined id, unique within the tenant and immutable once set.
        #[arg(long, value_parser = parse_custom_id)]
        custom_id: Option<String>,
        #[command(flatten)]
        password: PasswordArgs,
    },
    /// Get a single connection. The password is never returned.
    Get {
        /// Connection id (or custom_id when `--by-custom-id` is set).
        id: String,
        /// Treat the positional argument as a caller-defined custom_id.
        #[arg(long)]
        by_custom_id: bool,
    },
    /// Update a connection; omitted fields stay unchanged.
    ///
    /// Changing --host, --port, --username, or --database means sending the
    /// password again, since the server cannot read back the stored one.
    Update {
        /// Connection id.
        id: String,
        /// New display name.
        #[arg(long, value_parser = parse_non_empty)]
        name: Option<String>,
        /// New description.
        #[arg(long)]
        description: Option<String>,
        /// New server host.
        #[arg(long, value_parser = parse_non_empty)]
        host: Option<String>,
        /// New server port.
        #[arg(long, value_parser = clap::value_parser!(u16).range(1..))]
        port: Option<u16>,
        /// New user name.
        #[arg(long, value_parser = parse_non_empty)]
        username: Option<String>,
        /// New database name.
        #[arg(long, value_parser = parse_non_empty)]
        database: Option<String>,
        #[command(flatten)]
        password: PasswordArgs,
    },
    /// Delete a connection.
    ///
    /// Refused while any datasource still reads through it. There is no
    /// confirmation prompt.
    Delete {
        /// Connection id.
        id: String,
    },
    /// Check credentials without saving them, and list the schemas they can
    /// read.
    Test {
        /// Server host name or address.
        #[arg(long, value_parser = parse_non_empty)]
        host: String,
        /// Server port. The server defaults to 5432.
        #[arg(long, value_parser = clap::value_parser!(u16).range(1..))]
        port: Option<u16>,
        /// User name to authenticate with.
        #[arg(long, value_parser = parse_non_empty)]
        username: String,
        /// Name of the database to read.
        #[arg(long, value_parser = parse_non_empty)]
        database: String,
        #[command(flatten)]
        password: PasswordArgs,
    },
    /// List the schemas a saved connection can read: the choices for
    /// `datasource create --schema`.
    Schemas {
        /// Connection id.
        id: String,
    },
}

/// Execute a `db-connection` subcommand.
///
/// Everything that can be checked locally — including reading the password —
/// happens before credentials are resolved, so a malformed command fails the
/// same way whether or not the user is logged in.
pub fn run(
    command: DbConnectionCommand,
    profile: Option<String>,
    base_url: Option<String>,
) -> Result<()> {
    let connect = || api_client(profile, base_url);

    match command {
        DbConnectionCommand::List {
            name_fuzzy,
            page_size,
            continuation_token,
        } => {
            let data = list_db_connections(
                &connect()?,
                &ListDbConnectionsParams {
                    page_size,
                    continuation_token,
                    name_fuzzy,
                },
            )
            .context("list database connections")?;
            print_json(&data)
        }
        DbConnectionCommand::Create {
            name,
            host,
            port,
            username,
            database,
            description,
            custom_id,
            password,
        } => {
            let password = required_password(&password, "creating a connection needs a password")?;
            let request = CreateDbConnectionRequest {
                name,
                description,
                host,
                port,
                username,
                password,
                database,
                custom_id,
            };
            let data = create_db_connection(&connect()?, &request)
                .map_err(|err| {
                    let hint = is_internal_error(&err);
                    let err = anyhow::Error::new(err);
                    if hint {
                        err.context(
                            "create database connection (the server reported an internal \
                             error; `memorylake db-connection test` with the same host, port, \
                             username, database, and password source tells a credential \
                             problem from a server-side one)",
                        )
                    } else {
                        err.context("create database connection")
                    }
                })
                .map_err(|err| redact_password(err, Some(&request.password)))?;
            print_json(&data)
        }
        DbConnectionCommand::Get { id, by_custom_id } => {
            let client = connect()?;
            let data = if by_custom_id {
                get_db_connection_by_custom_id(&client, &id)
                    .with_context(|| format!("get database connection by custom_id `{id}`"))?
            } else {
                get_db_connection(&client, &id)
                    .with_context(|| format!("get database connection `{id}`"))?
            };
            print_json(&data)
        }
        DbConnectionCommand::Update {
            id,
            name,
            description,
            host,
            port,
            username,
            database,
            password,
        } => {
            let mut request = UpdateDbConnectionRequest {
                name,
                description,
                host,
                port,
                username,
                password: None,
                database,
            };
            let need = if request.changes_credentials() {
                PasswordNeed::Required
            } else {
                PasswordNeed::Optional
            };
            request.password = password.resolve(
                need,
                "changing --host, --port, --username, or --database needs the password again \
                 (the server never returns the stored one)",
            )?;
            if request.is_empty() {
                bail!(
                    "nothing to update: pass at least one of --name, --description, --host, \
                     --port, --username, --database, or a password source"
                );
            }
            let data = update_db_connection(&connect()?, &id, &request)
                .with_context(|| format!("update database connection `{id}`"))
                .map_err(|err| redact_password(err, request.password.as_ref()))?;
            print_json(&data)
        }
        DbConnectionCommand::Delete { id } => {
            delete_db_connection(&connect()?, &id)
                .with_context(|| format!("delete database connection `{id}`"))?;
            println!("Deleted database connection `{id}`");
            Ok(())
        }
        DbConnectionCommand::Test {
            host,
            port,
            username,
            database,
            password,
        } => {
            let password = required_password(&password, "testing a connection needs a password")?;
            let request = ConnectivityTestRequest {
                host,
                port,
                username,
                password,
                database,
            };
            let data = test_db_connectivity(&connect()?, &request)
                .context("test database connectivity")
                .map_err(|err| redact_password(err, Some(&request.password)))?;
            print_json(&data)
        }
        DbConnectionCommand::Schemas { id } => {
            let data = list_db_connection_schemas(&connect()?, &id)
                .with_context(|| format!("list schemas of database connection `{id}`"))?;
            print_json(&data)
        }
    }
}

/// Read a password the command cannot do without.
fn required_password(args: &PasswordArgs, missing: &str) -> Result<DbPassword> {
    match args.resolve(PasswordNeed::Required, missing)? {
        Some(password) => Ok(password),
        // `resolve` either yields a password or fails when one is required.
        None => bail!("{missing}"),
    }
}

/// Replace the password wherever it appears in an error's messages.
///
/// The request body is already redacted from traces, but the server's error
/// message may quote the database driver, and a driver error can echo the
/// password it was handed. The chain is only rebuilt (as plain messages) when
/// some message actually contains it.
fn redact_password(err: anyhow::Error, password: Option<&DbPassword>) -> anyhow::Error {
    let Some(secret) = password.map(DbPassword::expose).filter(|s| !s.is_empty()) else {
        return err;
    };
    let messages: Vec<String> = err.chain().map(ToString::to_string).collect();
    if !messages.iter().any(|message| message.contains(secret)) {
        return err;
    }
    let mut messages = messages
        .into_iter()
        .rev()
        .map(|message| message.replace(secret, "REDACTED"));
    let innermost = messages.next().unwrap_or_default();
    messages.fold(anyhow!(innermost), |err, outer| err.context(outer))
}

/// Whether the server reported a bare internal error.
fn is_internal_error(err: &CoreError) -> bool {
    matches!(err, CoreError::Api { code: Some(code), .. } if code == "INTERNAL_ERROR")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_password_in_any_error_message_is_redacted() {
        let password = DbPassword::new("hunter2");
        let err =
            anyhow!("server said: password hunter2 rejected").context("create database connection");
        let redacted = redact_password(err, Some(&password));
        let text = format!("{redacted:#}");
        assert!(!text.contains("hunter2"), "{text}");
        assert_eq!(
            text,
            "create database connection: server said: password REDACTED rejected"
        );
    }

    #[test]
    fn an_error_without_the_password_is_left_alone() {
        let password = DbPassword::new("hunter2");
        let err = anyhow!("connection refused").context("test database connectivity");
        let text = format!("{:#}", redact_password(err, Some(&password)));
        assert_eq!(text, "test database connectivity: connection refused");
    }
}
