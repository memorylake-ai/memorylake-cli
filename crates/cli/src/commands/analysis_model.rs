//! `memorylake analysis-model` / `am` commands.
//!
//! An analysis model is curated knowledge about one database datasource —
//! business rules, worked question-to-SQL examples, metric definitions and
//! background notes — kept as *entries*. Each model follows a template (its
//! `type`), and the template decides which entry kinds (`entity_type`s) it
//! accepts; `analysis-model templates` lists them.

use std::path::PathBuf;

use anyhow::{Context, Result, bail};
use clap::{Args, Subcommand, ValueEnum};
use memorylake_core::api::analysis_models::{
    CreateAnalysisModelRequest, CreateEntryRequest, ListAnalysisModelsParams, ListEntriesParams,
    SetEntryDisabledRequest, UpdateAnalysisModelRequest, UpdateEntryRequest, create_analysis_model,
    create_entry, delete_analysis_model, delete_entry, get_analysis_model,
    get_analysis_model_by_custom_id, get_entry, list_analysis_model_templates,
    list_analysis_models, list_entries, set_entry_disabled, update_analysis_model, update_entry,
};
use memorylake_core::{Client, Paths, ResolveOverrides, resolve};
use serde_json::{Map, Value};

use super::search::{IdList, parse_id_list};
use super::{print_json, require_workspace};

/// A JSON object given on the command line.
type JsonObject = Map<String, Value>;

/// Analysis model subcommands.
///
/// Every subcommand acts inside a workspace: `--workspace`, or the one
/// remembered by `workspace use`.
#[derive(Debug, Subcommand)]
pub enum AnalysisModelCommand {
    /// List the model templates and the entry kinds each accepts.
    ///
    /// Use a template's `type` with `create --type`, and its `entity_types[].code`
    /// values with `entry ... --entity-type`.
    Templates {
        /// Workspace to ask in. The answer is the same for every workspace.
        ///
        /// Defaults to the workspace remembered by `workspace use`.
        #[arg(long)]
        workspace: Option<String>,
        /// Only the template with this type (e.g. `ASK_DATA`).
        #[arg(long = "type", value_name = "TYPE")]
        model_type: Option<String>,
    },
    /// List the analysis models in a workspace.
    ///
    /// `--type` is applied after the server takes a page, so a page can come
    /// back short or even empty while still carrying a continuation token.
    List {
        /// Workspace id that owns the models.
        ///
        /// Defaults to the workspace remembered by `workspace use`.
        #[arg(long)]
        workspace: Option<String>,
        /// Only models built on this database datasource.
        #[arg(long, alias = "db-datasource", value_name = "DATASOURCE_ID")]
        datasource: Option<String>,
        /// Only models of this template type.
        #[arg(long = "type", value_name = "TYPE")]
        model_type: Option<String>,
        /// Number of items per page (1-100).
        #[arg(long, value_parser = clap::value_parser!(u32).range(1..=100))]
        page_size: Option<u32>,
        /// Continuation token from a previous response.
        #[arg(long)]
        continuation_token: Option<String>,
    },
    /// Create an analysis model on a database datasource.
    ///
    /// With `--fork-from`, the source model's knowledge is copied in the
    /// background: the new model comes back empty and fills in shortly after.
    Create {
        /// Workspace id to create the model in.
        ///
        /// Defaults to the workspace remembered by `workspace use`.
        #[arg(long)]
        workspace: Option<String>,
        /// Display name.
        #[arg(long)]
        name: String,
        /// Template to follow (e.g. `ASK_DATA`); see `analysis-model templates`.
        #[arg(long = "type", value_name = "TYPE")]
        model_type: String,
        /// Database datasource to build on. Must be in the same workspace.
        /// Cannot be changed later.
        #[arg(long, alias = "db-datasource", value_name = "DATASOURCE_ID")]
        datasource: String,
        /// Free-form description.
        #[arg(long)]
        description: Option<String>,
        /// Caller-defined id, unique within the tenant. Cannot be changed later,
        /// and may not start with `_sys_`.
        #[arg(long)]
        custom_id: Option<String>,
        /// Start from an existing model of the same type in this workspace.
        /// A fork cannot itself be forked.
        #[arg(long, value_name = "MODEL_ID")]
        fork_from: Option<String>,
    },
    /// Get one analysis model.
    Get {
        /// Workspace id that owns the model.
        ///
        /// Defaults to the workspace remembered by `workspace use`.
        #[arg(long)]
        workspace: Option<String>,
        /// Model id (or custom_id when `--by-custom-id` is set).
        id: String,
        /// Treat the positional argument as a caller-defined custom_id.
        #[arg(long)]
        by_custom_id: bool,
    },
    /// Rename a model or change its description.
    ///
    /// Only the flags you pass are sent. Type, datasource and custom id are
    /// fixed at creation.
    Update {
        /// Workspace id that owns the model.
        ///
        /// Defaults to the workspace remembered by `workspace use`.
        #[arg(long)]
        workspace: Option<String>,
        /// Model id.
        id: String,
        /// New display name.
        #[arg(long)]
        name: Option<String>,
        /// New description.
        #[arg(long)]
        description: Option<String>,
    },
    /// Permanently delete a model and all of its knowledge.
    ///
    /// Refused while a database memory still uses the model. There is no
    /// confirmation prompt.
    Delete {
        /// Workspace id that owns the model.
        ///
        /// Defaults to the workspace remembered by `workspace use`.
        #[arg(long)]
        workspace: Option<String>,
        /// Model id.
        id: String,
    },
    /// Manage the knowledge entries a model holds.
    Entry {
        #[command(subcommand)]
        command: EntryCommand,
    },
}

/// Workspace and model shared by every `entry` subcommand.
#[derive(Debug, Args)]
pub struct ModelScope {
    /// Workspace id that owns the model.
    ///
    /// Defaults to the workspace remembered by `workspace use`.
    #[arg(long)]
    workspace: Option<String>,
    /// Analysis model id.
    #[arg(long, value_name = "MODEL_ID")]
    model: String,
}

/// Arguments shared by `entry disable` and `entry enable`.
#[derive(Debug, Args)]
pub struct ToggleArgs {
    #[command(flatten)]
    scope: ModelScope,
    /// Entry id.
    id: String,
    /// Kind the entry was created as. Some kinds cannot be found by id alone.
    #[arg(long, value_name = "KIND")]
    entity_type: Option<String>,
}

/// Where an entry came from, for `entry list --from`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, ValueEnum)]
pub enum EntryOrigin {
    /// Written by hand.
    #[value(name = "MANUAL")]
    Manual,
    /// Produced when the model was built.
    #[value(name = "BUILD")]
    Build,
}

impl EntryOrigin {
    /// Wire value of this origin.
    fn as_str(self) -> &'static str {
        match self {
            Self::Manual => "MANUAL",
            Self::Build => "BUILD",
        }
    }
}

/// Knowledge entry subcommands.
#[derive(Debug, Subcommand)]
pub enum EntryCommand {
    /// List one kind of knowledge in a model.
    ///
    /// A model lists one kind at a time, so `--entity-type` is required. With
    /// `--keyword` the list is a similarity ranking, not a filter: it returns
    /// the closest entries even when nothing matches, so read each `score`.
    List {
        #[command(flatten)]
        scope: ModelScope,
        /// Kind of knowledge to list (e.g. `few_shot`, `biz_rule`, `general`,
        /// `drilldown_entity`).
        #[arg(long, value_name = "KIND")]
        entity_type: String,
        /// Rank entries by similarity to this text. Disabled entries are left
        /// out.
        #[arg(long)]
        keyword: Option<String>,
        /// Only entries written by hand (MANUAL) or produced by a build (BUILD).
        #[arg(long = "from", value_enum, value_name = "ORIGIN")]
        origin: Option<EntryOrigin>,
        /// Only these entries, comma-separated ids.
        #[arg(long, value_name = "IDS", value_parser = parse_id_list)]
        ids: Option<IdList>,
        /// Number of items per page (1-100).
        #[arg(long, value_parser = clap::value_parser!(u32).range(1..=100))]
        page_size: Option<u32>,
        /// Continuation token from a previous response.
        #[arg(long)]
        continuation_token: Option<String>,
    },
    /// Get one entry.
    Get {
        #[command(flatten)]
        scope: ModelScope,
        /// Entry id.
        id: String,
        /// Kind the entry was created as. Some kinds cannot be found by id
        /// alone.
        #[arg(long, value_name = "KIND")]
        entity_type: Option<String>,
    },
    /// Add one entry of knowledge to a model.
    ///
    /// The payload is a JSON object whose shape follows the kind, e.g. for a
    /// worked example:
    /// '{"artifact":{"question":{"type":"TEXT","content":"..."},
    /// "few_shot":{"type":"SQL","content":"SELECT ..."}}}'.
    /// A worked example's SQL is checked against the datasource's tables.
    Create {
        #[command(flatten)]
        scope: ModelScope,
        /// Kind of knowledge (e.g. `few_shot`, `biz_rule`, `general`,
        /// `drilldown_entity`); see `analysis-model templates`.
        #[arg(long, value_name = "KIND")]
        entity_type: String,
        /// Text incoming questions are matched against. Required for every kind
        /// except `biz_rule`, which derives it from the rule when omitted.
        #[arg(long)]
        embedding: Option<String>,
        /// Entry content as an inline JSON object.
        #[arg(
            long,
            value_name = "JSON",
            value_parser = parse_json_object,
            required_unless_present = "payload_file",
            conflicts_with = "payload_file"
        )]
        payload: Option<JsonObject>,
        /// Entry content read from a file holding a JSON object.
        #[arg(long, value_name = "PATH", value_parser = parse_json_object_file)]
        payload_file: Option<JsonObject>,
        /// Caller data kept alongside the entry, as a JSON object. A business
        /// rule on a document-library model needs `doc_id` here.
        #[arg(long, value_name = "JSON", value_parser = parse_json_object)]
        extra: Option<JsonObject>,
        /// Add the entry already excluded from answering.
        #[arg(long)]
        disabled: bool,
    },
    /// Edit one entry. Only the flags you pass are changed.
    ///
    /// `--payload` and `--extra` each REPLACE the stored value as a whole —
    /// include every slot or key you want to keep. Use `entry disable` /
    /// `entry enable` to take an entry out of use or put it back.
    Update {
        #[command(flatten)]
        scope: ModelScope,
        /// Entry id.
        id: String,
        /// Kind the entry was created as. Required: the server routes the edit
        /// by it.
        #[arg(long, value_name = "KIND")]
        entity_type: String,
        /// Replacement match text.
        #[arg(long)]
        embedding: Option<String>,
        /// Replacement content as an inline JSON object.
        #[arg(
            long,
            value_name = "JSON",
            value_parser = parse_json_object,
            conflicts_with = "payload_file"
        )]
        payload: Option<JsonObject>,
        /// Replacement content read from a file holding a JSON object.
        #[arg(long, value_name = "PATH", value_parser = parse_json_object_file)]
        payload_file: Option<JsonObject>,
        /// Replacement caller data as a JSON object; `{}` clears it.
        #[arg(long, value_name = "JSON", value_parser = parse_json_object)]
        extra: Option<JsonObject>,
    },
    /// Permanently delete one entry.
    ///
    /// Some kinds may only be deleted when nothing else hangs off the entry
    /// (see `delete_validation` in `analysis-model templates`).
    Delete {
        #[command(flatten)]
        scope: ModelScope,
        /// Entry id.
        id: String,
        /// Kind the entry was created as. Some kinds cannot be found by id
        /// alone.
        #[arg(long, value_name = "KIND")]
        entity_type: Option<String>,
    },
    /// Exclude an entry from answering without deleting it.
    Disable(ToggleArgs),
    /// Put a disabled entry back into use.
    Enable(ToggleArgs),
}

/// Execute an `analysis-model` subcommand.
pub fn run(
    command: AnalysisModelCommand,
    profile: Option<String>,
    base_url: Option<String>,
) -> Result<()> {
    // Reject unusable input before resolving credentials so a malformed command
    // fails the same way whether or not the user is logged in.
    validate(&command)?;

    let paths = Paths::default_home().context("resolve MemoryLake config paths")?;
    let runtime = resolve(&paths, &ResolveOverrides { profile, base_url })
        .context("resolve API credentials")?;
    let client = Client::new(&runtime.base_url, &runtime.api_key).context("build API client")?;
    let workspace_for = |flag: Option<String>| require_workspace(&paths, &runtime.profile, flag);

    match command {
        AnalysisModelCommand::Templates {
            workspace,
            model_type,
        } => {
            let workspace = workspace_for(workspace)?;
            let data = list_analysis_model_templates(&client, &workspace, model_type.as_deref())
                .context("list analysis model templates")?;
            print_json(&data)
        }
        AnalysisModelCommand::List {
            workspace,
            datasource,
            model_type,
            page_size,
            continuation_token,
        } => {
            let workspace = workspace_for(workspace)?;
            let params = ListAnalysisModelsParams {
                db_datasource_id: datasource,
                model_type,
                page_size,
                continuation_token,
            };
            let data = list_analysis_models(&client, &workspace, &params)
                .context("list analysis models")?;
            print_json(&data)
        }
        AnalysisModelCommand::Create {
            workspace,
            name,
            model_type,
            datasource,
            description,
            custom_id,
            fork_from,
        } => {
            let workspace = workspace_for(workspace)?;
            let request = CreateAnalysisModelRequest {
                name,
                model_type,
                db_datasource_id: datasource,
                description,
                custom_id,
                fork_from,
            };
            let data = create_analysis_model(&client, &workspace, &request)
                .context("create analysis model")?;
            print_json(&data)
        }
        AnalysisModelCommand::Get {
            workspace,
            id,
            by_custom_id,
        } => {
            let workspace = workspace_for(workspace)?;
            let data = if by_custom_id {
                get_analysis_model_by_custom_id(&client, &workspace, &id)
                    .with_context(|| format!("get analysis model by custom_id `{id}`"))?
            } else {
                get_analysis_model(&client, &workspace, &id)
                    .with_context(|| format!("get analysis model `{id}`"))?
            };
            print_json(&data)
        }
        AnalysisModelCommand::Update {
            workspace,
            id,
            name,
            description,
        } => {
            let workspace = workspace_for(workspace)?;
            let request = UpdateAnalysisModelRequest { name, description };
            let data = update_analysis_model(&client, &workspace, &id, &request)
                .with_context(|| format!("update analysis model `{id}`"))?;
            print_json(&data)
        }
        AnalysisModelCommand::Delete { workspace, id } => {
            let workspace = workspace_for(workspace)?;
            delete_analysis_model(&client, &workspace, &id)
                .with_context(|| format!("delete analysis model `{id}`"))?;
            println!("Deleted analysis model `{id}` in workspace `{workspace}`");
            Ok(())
        }
        AnalysisModelCommand::Entry { command } => run_entry(&client, &workspace_for, command),
    }
}

/// Execute an `analysis-model entry` subcommand.
fn run_entry(
    client: &Client,
    workspace_for: &dyn Fn(Option<String>) -> Result<String>,
    command: EntryCommand,
) -> Result<()> {
    match command {
        EntryCommand::List {
            scope,
            entity_type,
            keyword,
            origin,
            ids,
            page_size,
            continuation_token,
        } => {
            let workspace = workspace_for(scope.workspace)?;
            let params = ListEntriesParams {
                keyword,
                origin: origin.map(|origin| origin.as_str().to_string()),
                ids: ids.map(|list| list.0).unwrap_or_default(),
                page_size,
                continuation_token,
                ..ListEntriesParams::new(entity_type)
            };
            let data = list_entries(client, &workspace, &scope.model, &params)
                .with_context(|| format!("list entries of analysis model `{}`", scope.model))?;
            print_json(&data)
        }
        EntryCommand::Get {
            scope,
            id,
            entity_type,
        } => {
            let workspace = workspace_for(scope.workspace)?;
            let data = get_entry(
                client,
                &workspace,
                &scope.model,
                &id,
                entity_type.as_deref(),
            )
            .with_context(|| format!("get entry `{id}`"))?;
            print_json(&data)
        }
        EntryCommand::Create {
            scope,
            entity_type,
            embedding,
            payload,
            payload_file,
            extra,
            disabled,
        } => {
            let workspace = workspace_for(scope.workspace)?;
            let Some(payload) = payload.or(payload_file) else {
                // clap requires one of the two; this only guards that rule.
                bail!("`entry create` requires --payload or --payload-file");
            };
            let request = CreateEntryRequest {
                entity_type,
                embedding,
                payload,
                extra,
                disabled: disabled.then_some(true),
            };
            let data = create_entry(client, &workspace, &scope.model, &request)
                .with_context(|| format!("add entry to analysis model `{}`", scope.model))?;
            print_json(&data)
        }
        EntryCommand::Update {
            scope,
            id,
            entity_type,
            embedding,
            payload,
            payload_file,
            extra,
        } => {
            let workspace = workspace_for(scope.workspace)?;
            let request = UpdateEntryRequest {
                embedding,
                payload: payload.or(payload_file),
                extra,
                ..UpdateEntryRequest::new(entity_type)
            };
            let data = update_entry(client, &workspace, &scope.model, &id, &request)
                .with_context(|| format!("update entry `{id}`"))?;
            print_json(&data)
        }
        EntryCommand::Delete {
            scope,
            id,
            entity_type,
        } => {
            let workspace = workspace_for(scope.workspace)?;
            delete_entry(
                client,
                &workspace,
                &scope.model,
                &id,
                entity_type.as_deref(),
            )
            .with_context(|| format!("delete entry `{id}`"))?;
            println!("Deleted entry `{id}` from analysis model `{}`", scope.model);
            Ok(())
        }
        EntryCommand::Disable(args) => toggle_entry(client, workspace_for, args, true),
        EntryCommand::Enable(args) => toggle_entry(client, workspace_for, args, false),
    }
}

/// Run `entry disable` (`disabled = true`) or `entry enable` (`false`).
fn toggle_entry(
    client: &Client,
    workspace_for: &dyn Fn(Option<String>) -> Result<String>,
    args: ToggleArgs,
    disabled: bool,
) -> Result<()> {
    let ToggleArgs {
        scope,
        id,
        entity_type,
    } = args;
    let workspace = workspace_for(scope.workspace)?;
    let request = SetEntryDisabledRequest {
        disabled,
        entity_type,
    };
    let verb = if disabled { "Disabled" } else { "Enabled" };
    set_entry_disabled(client, &workspace, &scope.model, &id, &request)
        .with_context(|| format!("{} entry `{id}`", verb.to_lowercase()))?;
    println!("{verb} entry `{id}` of analysis model `{}`", scope.model);
    Ok(())
}

/// Reject commands that cannot produce a meaningful request.
fn validate(command: &AnalysisModelCommand) -> Result<()> {
    match command {
        AnalysisModelCommand::Update {
            name, description, ..
        } if name.is_none() && description.is_none() => {
            bail!("`analysis-model update` requires at least one of --name or --description")
        }
        AnalysisModelCommand::Entry {
            command:
                EntryCommand::Update {
                    embedding,
                    payload,
                    payload_file,
                    extra,
                    ..
                },
        } if embedding.is_none()
            && payload.is_none()
            && payload_file.is_none()
            && extra.is_none() =>
        {
            bail!(
                "`analysis-model entry update` requires at least one of --embedding, --payload, --payload-file, or --extra"
            )
        }
        _ => Ok(()),
    }
}

/// Parse a flag value as a JSON object.
///
/// Rejects malformed JSON and valid JSON that is not an object, so an invalid
/// value never reaches the API.
fn parse_json_object(raw: &str) -> std::result::Result<JsonObject, String> {
    let value: Value = serde_json::from_str(raw)
        .map_err(|err| format!("must be a JSON object: invalid JSON: {err}"))?;
    match value {
        Value::Object(map) => Ok(map),
        other => Err(format!("must be a JSON object, got {}", json_kind(&other))),
    }
}

/// Read a file and parse its contents as a JSON object.
///
/// Runs as a clap value parser, so a missing or malformed file is reported
/// while arguments are parsed — before any credentials are looked up.
fn parse_json_object_file(raw: &str) -> std::result::Result<JsonObject, String> {
    let path = PathBuf::from(raw);
    let text = std::fs::read_to_string(&path)
        .map_err(|err| format!("cannot read {}: {err}", path.display()))?;
    parse_json_object(&text).map_err(|err| format!("{}: {err}", path.display()))
}

/// Name a JSON value's kind for error messages.
fn json_kind(value: &Value) -> &'static str {
    match value {
        Value::Null => "null",
        Value::Bool(_) => "a boolean",
        Value::Number(_) => "a number",
        Value::String(_) => "a string",
        Value::Array(_) => "an array",
        Value::Object(_) => "an object",
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_json_object_accepts_an_object() {
        let map = parse_json_object(r#"{"artifact":{}}"#).expect("valid object");
        assert!(map.contains_key("artifact"));
    }

    #[test]
    fn parse_json_object_rejects_non_objects() {
        for (raw, kind) in [("[1]", "an array"), ("\"x\"", "a string"), ("null", "null")] {
            assert_eq!(
                parse_json_object(raw).expect_err("non-object must be rejected"),
                format!("must be a JSON object, got {kind}")
            );
        }
        let err = parse_json_object("{").expect_err("malformed JSON must be rejected");
        assert!(err.contains("invalid JSON"), "{err}");
    }

    #[test]
    fn parse_json_object_file_names_the_missing_file() {
        let err = parse_json_object_file("/definitely/not/here.json")
            .expect_err("a missing file must be rejected");
        assert!(err.contains("/definitely/not/here.json"), "{err}");
    }

    #[test]
    fn entry_origin_maps_to_wire_values() {
        assert_eq!(EntryOrigin::Manual.as_str(), "MANUAL");
        assert_eq!(EntryOrigin::Build.as_str(), "BUILD");
    }

    #[test]
    fn validate_rejects_an_empty_model_update() {
        let command = AnalysisModelCommand::Update {
            workspace: None,
            id: "am-1".into(),
            name: None,
            description: None,
        };
        let err = validate(&command).expect_err("an empty update must be rejected");
        assert!(err.to_string().contains("--name or --description"), "{err}");
    }

    #[test]
    fn validate_rejects_an_entry_update_that_only_names_its_kind() {
        let command = AnalysisModelCommand::Entry {
            command: EntryCommand::Update {
                scope: ModelScope {
                    workspace: None,
                    model: "am-1".into(),
                },
                id: "e-1".into(),
                entity_type: "few_shot".into(),
                embedding: None,
                payload: None,
                payload_file: None,
                extra: None,
            },
        };
        let err = validate(&command).expect_err("a kind alone changes nothing");
        assert!(err.to_string().contains("--embedding"), "{err}");
    }

    #[test]
    fn validate_accepts_an_entry_update_with_an_empty_extra() {
        // `--extra '{}'` clears the stored value, which is a real change.
        let command = AnalysisModelCommand::Entry {
            command: EntryCommand::Update {
                scope: ModelScope {
                    workspace: None,
                    model: "am-1".into(),
                },
                id: "e-1".into(),
                entity_type: "few_shot".into(),
                embedding: None,
                payload: None,
                payload_file: None,
                extra: Some(Map::new()),
            },
        };
        validate(&command).expect("an empty extra is a real update");
    }
}
