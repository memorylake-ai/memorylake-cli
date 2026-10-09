//! `memorylake datasource` / `ds` commands.
//!
//! A datasource brings one schema of a database connection (`db-connection`)
//! into a workspace and indexes it. Project database memories
//! (`project database`) read through a datasource.

use std::path::PathBuf;

use anyhow::{Context, Result, bail};
use clap::{ArgAction, Subcommand};
use memorylake_core::api::db_datasources::{
    CreateDbDatasourceRequest, ListDbDatasourcesParams, MAX_SCHEMA_METADATA_EDITS,
    SchemaMetadataEdit, UpdateDbDatasourceRequest, UpdateSchemaMetadataRequest,
    build_db_datasource, create_db_datasource, delete_db_datasource, get_db_datasource,
    get_db_datasource_by_custom_id, list_db_datasource_columns, list_db_datasource_tables,
    list_db_datasources, update_db_datasource, update_db_datasource_schema_metadata,
};
use memorylake_core::{Client, Paths, ResolveOverrides, resolve};

use super::input::{parse_custom_id, read_file_or_stdin};
use super::{print_json, require_workspace};

/// Datasource subcommands.
///
/// Datasources live in a workspace: every subcommand takes `--workspace`,
/// defaulting to the one remembered by `workspace use`.
#[derive(Debug, Subcommand)]
pub enum DatasourceCommand {
    /// List the datasources in a workspace.
    List {
        /// Workspace id. Defaults to the workspace remembered by `workspace use`.
        #[arg(long)]
        workspace: Option<String>,
        /// Only datasources reading through this connection.
        #[arg(long, value_name = "CONNECTION_ID")]
        connection: Option<String>,
        /// Only datasources whose name contains this text (case-insensitive).
        #[arg(long = "name")]
        name_fuzzy: Option<String>,
        /// Number of items per page (1-100).
        #[arg(long, value_parser = clap::value_parser!(u32).range(1..=100))]
        page_size: Option<u32>,
        /// Continuation token from a previous response.
        #[arg(long)]
        continuation_token: Option<String>,
    },
    /// Create a datasource over one schema of a connection.
    ///
    /// Creating it starts the first index build. Poll `datasource get` until
    /// `building_version` is empty to know when it has finished. The
    /// connection and the schema cannot be changed afterwards.
    Create {
        /// Workspace id. Defaults to the workspace remembered by `workspace use`.
        #[arg(long)]
        workspace: Option<String>,
        /// Connection to read through (see `db-connection list`).
        #[arg(long, value_name = "CONNECTION_ID")]
        connection: String,
        /// Schema to cover (see `db-connection schemas`). Exactly one for now.
        #[arg(long)]
        schema: String,
        /// Display name.
        #[arg(long)]
        name: String,
        /// Free-form description.
        #[arg(long)]
        description: Option<String>,
        /// Caller-defined id, unique within the tenant and immutable once set.
        #[arg(long, value_parser = parse_custom_id)]
        custom_id: Option<String>,
        /// Regular expression selecting which tables are indexed. The server
        /// defaults to `.*` (every table).
        #[arg(long, value_name = "REGEX")]
        table_filter: Option<String>,
    },
    /// Get a datasource, including its schemas and build state.
    ///
    /// A non-empty `building_version` means an index build is running.
    Get {
        /// Workspace id. Defaults to the workspace remembered by `workspace use`.
        #[arg(long)]
        workspace: Option<String>,
        /// Datasource id (or custom_id when `--by-custom-id` is set).
        id: String,
        /// Treat the positional argument as a caller-defined custom_id.
        #[arg(long)]
        by_custom_id: bool,
    },
    /// Update a datasource's name, description, or table filter.
    ///
    /// Only the flags you pass are sent. A new table filter takes effect on
    /// the next `datasource build`.
    Update {
        /// Workspace id. Defaults to the workspace remembered by `workspace use`.
        #[arg(long)]
        workspace: Option<String>,
        /// Datasource id.
        id: String,
        /// New display name.
        #[arg(long)]
        name: Option<String>,
        /// New description.
        #[arg(long)]
        description: Option<String>,
        /// New table filter regular expression.
        #[arg(long, value_name = "REGEX")]
        table_filter: Option<String>,
    },
    /// Delete a datasource.
    ///
    /// Refused while any database memory or analysis model still uses it.
    /// There is no confirmation prompt.
    Delete {
        /// Workspace id. Defaults to the workspace remembered by `workspace use`.
        #[arg(long)]
        workspace: Option<String>,
        /// Datasource id.
        id: String,
    },
    /// Start a new index build.
    ///
    /// Runs in the background; poll `datasource get` until `building_version`
    /// is empty. Every run produces a new build version.
    Build {
        /// Workspace id. Defaults to the workspace remembered by `workspace use`.
        #[arg(long)]
        workspace: Option<String>,
        /// Datasource id.
        id: String,
    },
    /// List the tables a datasource covers.
    Tables {
        /// Workspace id. Defaults to the workspace remembered by `workspace use`.
        #[arg(long)]
        workspace: Option<String>,
        /// Datasource id.
        id: String,
        /// Only tables whose name contains this text (case-insensitive).
        #[arg(long = "name")]
        name_fuzzy: Option<String>,
    },
    /// List the columns of one table.
    Columns {
        /// Workspace id. Defaults to the workspace remembered by `workspace use`.
        #[arg(long)]
        workspace: Option<String>,
        /// Datasource id.
        id: String,
        /// Table whose columns to list.
        #[arg(long)]
        table: String,
    },
    /// Edit table and column annotations, and which columns are indexed.
    ///
    /// Either describe one edit with --table (plus --column for a column), or
    /// pass a batch of up to 200 as a JSON array with --edits / --edits-file:
    ///
    ///   [{"target":"table","table_name":"orders","comment":"One row per order"},
    ///    {"target":"column","table_name":"orders","column_name":"status",
    ///     "embedding_enabled":true}]
    ///
    /// An empty --comment clears the annotation. Indexing changes take effect
    /// on the next `datasource build`.
    Annotate {
        /// Workspace id. Defaults to the workspace remembered by `workspace use`.
        #[arg(long)]
        workspace: Option<String>,
        /// Datasource id.
        id: String,
        /// Table to edit.
        #[arg(long, conflicts_with_all = ["edits", "edits_file"], required_unless_present_any = ["edits", "edits_file"])]
        table: Option<String>,
        /// Column of --table to edit; without it the table itself is edited.
        #[arg(long, requires = "table")]
        column: Option<String>,
        /// Schema the table is in. Only needed once a datasource covers more
        /// than one schema.
        #[arg(long, requires = "table")]
        schema: Option<String>,
        /// New annotation; an empty string clears it.
        #[arg(long, requires = "table")]
        comment: Option<String>,
        /// Whether to index the column's values for semantic matching.
        #[arg(
            long,
            requires = "column",
            action = ArgAction::Set,
            value_parser = clap::value_parser!(bool),
            value_name = "true|false"
        )]
        embedding: Option<bool>,
        /// Edits as a JSON array.
        #[arg(long, value_name = "JSON", conflicts_with = "edits_file")]
        edits: Option<String>,
        /// File holding the edits as a JSON array; `-` reads standard input.
        #[arg(long, value_name = "PATH")]
        edits_file: Option<PathBuf>,
    },
}

/// Execute a `datasource` subcommand.
pub fn run(
    command: DatasourceCommand,
    profile: Option<String>,
    base_url: Option<String>,
) -> Result<()> {
    // Annotation edits are parsed (and their file read) before credentials are
    // resolved, so a malformed batch fails the same way logged in or not.
    let command = match command {
        DatasourceCommand::Annotate {
            workspace,
            id,
            table,
            column,
            schema,
            comment,
            embedding,
            edits,
            edits_file,
        } => {
            let parsed = match (table, edits, edits_file) {
                (Some(table_name), None, None) => {
                    vec![single_edit(table_name, column, schema, comment, embedding)?]
                }
                (None, Some(json), None) => parse_edits(&json, "--edits")?,
                (None, None, Some(path)) => {
                    let json = read_file_or_stdin(&path, "edits file")?;
                    parse_edits(&json, "--edits-file")?
                }
                // clap's conflicts / required_unless rules make these unreachable.
                _ => bail!("pass exactly one of --table, --edits, or --edits-file"),
            };
            Prepared::Annotate {
                workspace,
                id,
                items: parsed,
            }
        }
        DatasourceCommand::Update {
            workspace,
            id,
            name,
            description,
            table_filter,
        } => {
            let request = UpdateDbDatasourceRequest {
                name,
                description,
                table_filter_rule: table_filter,
            };
            if request.is_empty() {
                bail!(
                    "nothing to update: pass at least one of --name, --description, --table-filter"
                );
            }
            Prepared::Update {
                workspace,
                id,
                request,
            }
        }
        other => Prepared::Other(other),
    };

    let paths = Paths::default_home().context("resolve MemoryLake config paths")?;
    let runtime = resolve(&paths, &ResolveOverrides { profile, base_url })
        .context("resolve API credentials")?;
    let client = Client::new(&runtime.base_url, &runtime.api_key).context("build API client")?;
    let workspace_of = |flag: Option<String>| require_workspace(&paths, &runtime.profile, flag);

    match command {
        Prepared::Annotate {
            workspace,
            id,
            items,
        } => {
            let workspace = workspace_of(workspace)?;
            let count = items.len();
            update_db_datasource_schema_metadata(
                &client,
                &workspace,
                &id,
                &UpdateSchemaMetadataRequest { items },
            )
            .with_context(|| format!("edit schema metadata of datasource `{id}`"))?;
            println!("Applied {count} schema metadata edit(s) to datasource `{id}`");
            Ok(())
        }
        Prepared::Update {
            workspace,
            id,
            request,
        } => {
            let workspace = workspace_of(workspace)?;
            let data = update_db_datasource(&client, &workspace, &id, &request)
                .with_context(|| format!("update datasource `{id}`"))?;
            print_json(&data)
        }
        Prepared::Other(command) => run_simple(&client, workspace_of, command),
    }
}

/// A command whose local checks have already run.
enum Prepared {
    Annotate {
        workspace: Option<String>,
        id: String,
        items: Vec<SchemaMetadataEdit>,
    },
    Update {
        workspace: Option<String>,
        id: String,
        request: UpdateDbDatasourceRequest,
    },
    Other(DatasourceCommand),
}

/// Subcommands with nothing to check beyond what clap already did.
fn run_simple(
    client: &Client,
    workspace_of: impl Fn(Option<String>) -> Result<String>,
    command: DatasourceCommand,
) -> Result<()> {
    match command {
        DatasourceCommand::List {
            workspace,
            connection,
            name_fuzzy,
            page_size,
            continuation_token,
        } => {
            let workspace = workspace_of(workspace)?;
            let data = list_db_datasources(
                client,
                &workspace,
                &ListDbDatasourcesParams {
                    db_connection_id: connection,
                    name_fuzzy,
                    page_size,
                    continuation_token,
                },
            )
            .with_context(|| format!("list datasources in workspace `{workspace}`"))?;
            print_json(&data)
        }
        DatasourceCommand::Create {
            workspace,
            connection,
            schema,
            name,
            description,
            custom_id,
            table_filter,
        } => {
            let workspace = workspace_of(workspace)?;
            let data = create_db_datasource(
                client,
                &workspace,
                &CreateDbDatasourceRequest {
                    schemas: vec![schema],
                    name,
                    description,
                    db_connection_id: connection,
                    custom_id,
                    table_filter_rule: table_filter,
                },
            )
            .context("create datasource")?;
            print_json(&data)
        }
        DatasourceCommand::Get {
            workspace,
            id,
            by_custom_id,
        } => {
            let workspace = workspace_of(workspace)?;
            let data = if by_custom_id {
                get_db_datasource_by_custom_id(client, &workspace, &id)
                    .with_context(|| format!("get datasource by custom_id `{id}`"))?
            } else {
                get_db_datasource(client, &workspace, &id)
                    .with_context(|| format!("get datasource `{id}`"))?
            };
            print_json(&data)
        }
        DatasourceCommand::Delete { workspace, id } => {
            let workspace = workspace_of(workspace)?;
            delete_db_datasource(client, &workspace, &id)
                .with_context(|| format!("delete datasource `{id}`"))?;
            println!("Deleted datasource `{id}` in workspace `{workspace}`");
            Ok(())
        }
        DatasourceCommand::Build { workspace, id } => {
            let workspace = workspace_of(workspace)?;
            build_db_datasource(client, &workspace, &id)
                .with_context(|| format!("build datasource `{id}`"))?;
            println!(
                "Started an index build for datasource `{id}`; \
                 `memorylake datasource get {id}` shows `building_version` until it finishes"
            );
            Ok(())
        }
        DatasourceCommand::Tables {
            workspace,
            id,
            name_fuzzy,
        } => {
            let workspace = workspace_of(workspace)?;
            let data = list_db_datasource_tables(client, &workspace, &id, name_fuzzy.as_deref())
                .with_context(|| format!("list tables of datasource `{id}`"))?;
            print_json(&data)
        }
        DatasourceCommand::Columns {
            workspace,
            id,
            table,
        } => {
            let workspace = workspace_of(workspace)?;
            let data = list_db_datasource_columns(client, &workspace, &id, &table)
                .with_context(|| format!("list columns of `{table}` in datasource `{id}`"))?;
            print_json(&data)
        }
        // Prepared by `run` before credentials were resolved.
        DatasourceCommand::Annotate { .. } | DatasourceCommand::Update { .. } => {
            bail!("internal error: command was not prepared")
        }
    }
}

/// Build the one edit described by `--table` and friends.
fn single_edit(
    table_name: String,
    column: Option<String>,
    schema_name: Option<String>,
    comment: Option<String>,
    embedding: Option<bool>,
) -> Result<SchemaMetadataEdit> {
    let edit = match column {
        Some(column_name) => SchemaMetadataEdit::Column {
            table_name,
            column_name,
            schema_name,
            comment,
            embedding_enabled: embedding,
        },
        None => SchemaMetadataEdit::Table {
            table_name,
            schema_name,
            comment,
        },
    };
    if !edit.changes_something() {
        bail!("nothing to change: pass --comment, or --embedding together with --column");
    }
    Ok(edit)
}

/// Parse and check a JSON batch of edits.
fn parse_edits(json: &str, source: &str) -> Result<Vec<SchemaMetadataEdit>> {
    let edits: Vec<SchemaMetadataEdit> = serde_json::from_str(json).with_context(|| {
        format!(
            "{source} must be a JSON array of edits, each with `target` (`table` or `column`), \
             `table_name`, `column_name` for a column, and `comment` and/or `embedding_enabled`"
        )
    })?;
    if edits.is_empty() {
        bail!("{source} holds no edits");
    }
    if edits.len() > MAX_SCHEMA_METADATA_EDITS {
        bail!(
            "{source} holds {} edits; one request takes at most {MAX_SCHEMA_METADATA_EDITS}",
            edits.len()
        );
    }
    if let Some(index) = edits.iter().position(|edit| !edit.changes_something()) {
        bail!("{source}: edit {index} changes nothing (give it `comment` or `embedding_enabled`)");
    }
    Ok(edits)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_column_flag_makes_a_column_edit() {
        let edit = single_edit(
            "orders".into(),
            Some("status".into()),
            None,
            None,
            Some(true),
        )
        .expect("edit");
        assert!(matches!(edit, SchemaMetadataEdit::Column { .. }));
    }

    #[test]
    fn an_edit_without_a_change_is_refused() {
        assert!(single_edit("orders".into(), None, None, None, None).is_err());
    }

    #[test]
    fn batches_are_bounded() {
        assert!(parse_edits("[]", "--edits").is_err());
        let one = r#"{"target":"table","table_name":"t","comment":"c"}"#;
        let too_many = format!("[{}]", vec![one; MAX_SCHEMA_METADATA_EDITS + 1].join(","));
        let err = parse_edits(&too_many, "--edits").expect_err("over the cap");
        assert!(err.to_string().contains("at most 200"), "{err}");
        let max = format!("[{}]", vec![one; MAX_SCHEMA_METADATA_EDITS].join(","));
        assert_eq!(parse_edits(&max, "--edits").expect("at the cap").len(), 200);
    }

    #[test]
    fn a_no_op_edit_in_a_batch_is_named() {
        let json = r#"[{"target":"table","table_name":"t","comment":"c"},
                       {"target":"column","table_name":"t","column_name":"c"}]"#;
        let err = parse_edits(json, "--edits").expect_err("no-op");
        assert!(err.to_string().contains("edit 1"), "{err}");
    }
}
