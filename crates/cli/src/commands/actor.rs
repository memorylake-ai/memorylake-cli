//! `memorylake actor` commands.

use super::require_workspace;
use super::search::split_csv;
use anyhow::{Context, Result, bail};
use clap::{Args, Subcommand, ValueEnum};
use memorylake_core::api::actors::{
    ActorType, CreateActorRequest, ListActorsParams, UpdateActorRequest, bind_actor, create_actor,
    delete_actor, get_actor, get_actor_by_custom_id, list_actors, list_workspace_actors,
    unbind_actor, update_actor,
};
use memorylake_core::{Client, Paths, ResolveOverrides, resolve};
use serde_json::{Map, Value};

/// Actor type accepted on the command line.
///
/// Deliberately closed and case-sensitive so a typo is rejected here instead of
/// costing a round trip. Values returned by the API are handled leniently by
/// [`ActorType`], which is a separate concern.
#[derive(Debug, Clone, Copy, PartialEq, Eq, ValueEnum)]
pub enum ActorTypeArg {
    /// An end user interacting with your application.
    #[value(name = "HUMAN")]
    Human,
    /// An AI agent.
    #[value(name = "ASSISTANT")]
    Assistant,
}

impl From<ActorTypeArg> for ActorType {
    fn from(value: ActorTypeArg) -> Self {
        match value {
            ActorTypeArg::Human => Self::Human,
            ActorTypeArg::Assistant => Self::Assistant,
        }
    }
}

/// One `--tags` flag's worth of labels, already split and validated.
///
/// Wrapped for the same reason as `IdList`: clap's derive reads a bare
/// `Option<Vec<T>>` as a repeatable flag yielding one `T` per occurrence, which
/// does not match a parser that returns the whole list from a single value.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TagList(Vec<String>);

/// Workspace and actor pair shared by `bind` and `unbind`.
#[derive(Debug, Args)]
pub struct BindingArgs {
    /// Workspace id.
    ///
    /// Defaults to the workspace remembered by `workspace use`.
    #[arg(long)]
    pub workspace: Option<String>,
    /// Actor id.
    #[arg(long)]
    pub actor: String,
}

/// Actor subcommands.
#[derive(Debug, Subcommand)]
pub enum ActorCommand {
    /// List actors, or the actors bound to one workspace.
    List {
        /// Number of items per page.
        #[arg(long)]
        page_size: Option<u32>,
        /// Continuation token from a previous response.
        #[arg(long)]
        continuation_token: Option<String>,
        /// Filter by actor type.
        #[arg(long = "type", value_enum)]
        actor_type: Option<ActorTypeArg>,
        /// Fuzzy filter by display name (partial match).
        #[arg(long = "name")]
        display_name_fuzzy: Option<String>,
        /// Filter by tag, comma-separated, e.g. `vip,cn`.
        ///
        /// Several tags are combined with AND: only actors carrying every one
        /// are returned. Matching is exact and case-sensitive.
        #[arg(long, value_parser = parse_tag_list)]
        tags: Option<TagList>,
        /// List the actors bound to this workspace instead of all actors.
        ///
        /// Items are workspace bindings (`actor_id`, `bound_at`, ...), which is
        /// a different shape from the actor objects returned without this flag.
        #[arg(long)]
        workspace: Option<String>,
    },
    /// Create an actor.
    Create {
        /// Caller-defined external id. Must be unique account-wide.
        #[arg(long)]
        custom_id: String,
        /// Human-readable name shown in the console.
        #[arg(long)]
        display_name: String,
        /// Actor type. The server defaults to HUMAN.
        #[arg(long = "type", value_enum)]
        actor_type: Option<ActorTypeArg>,
        /// Optional description of the actor's role or purpose.
        #[arg(long)]
        description: Option<String>,
        /// Labels to attach, comma-separated, e.g. `vip,cn`.
        ///
        /// Up to 20, each 1-64 characters. Use these rather than --metadata for
        /// anything you want to filter on later.
        #[arg(long, value_parser = parse_tag_list)]
        tags: Option<TagList>,
        /// Metadata as a JSON object, e.g. '{"tier":"premium"}'.
        #[arg(long, value_parser = parse_metadata_object)]
        metadata: Option<Map<String, Value>>,
    },
    /// Get a single actor by id.
    Get {
        /// Actor id (or custom_id when `--by-custom-id` is set).
        id: String,
        /// Treat the positional argument as a caller-defined custom_id.
        #[arg(long)]
        by_custom_id: bool,
    },
    /// Update an actor. Only the fields you pass are changed.
    Update {
        /// Actor id.
        id: String,
        /// New display name.
        #[arg(long)]
        display_name: Option<String>,
        /// New description.
        #[arg(long)]
        description: Option<String>,
        /// New tags, comma-separated. REPLACES the actor's tags entirely —
        /// list every tag you want to keep, because the server does not merge.
        #[arg(long, value_parser = parse_tag_list, conflicts_with = "clear_tags")]
        tags: Option<TagList>,
        /// Remove every tag from the actor.
        #[arg(long)]
        clear_tags: bool,
        /// Metadata as a JSON object. REPLACES the stored metadata entirely —
        /// include every key you want to keep, because the server does not
        /// merge.
        #[arg(long, value_parser = parse_metadata_object)]
        metadata: Option<Map<String, Value>>,
    },
    /// Delete an actor. Irreversible; workspace bindings are removed too.
    Delete {
        /// Actor id.
        id: String,
    },
    /// Bind an existing actor to a workspace.
    Bind(BindingArgs),
    /// Unbind an actor from a workspace. The actor itself is kept.
    Unbind(BindingArgs),
}

/// Execute an `actor` subcommand.
pub fn run(command: ActorCommand, profile: Option<String>, base_url: Option<String>) -> Result<()> {
    // Reject unusable input before resolving credentials so a malformed command
    // fails the same way whether or not the user is logged in.
    validate(&command)?;

    let paths = Paths::default_home().context("resolve MemoryLake config paths")?;
    let runtime = resolve(&paths, &ResolveOverrides { profile, base_url })
        .context("resolve API credentials")?;
    let client = Client::new(&runtime.base_url, &runtime.api_key).context("build API client")?;

    match command {
        ActorCommand::List {
            page_size,
            continuation_token,
            actor_type,
            display_name_fuzzy,
            tags,
            workspace,
        } => {
            let params = ListActorsParams {
                page_size,
                continuation_token,
                actor_type: actor_type.map(ActorType::from),
                display_name_fuzzy,
                tags: tags.map(|list| list.0),
            };
            match workspace {
                Some(workspace_id) => {
                    let data = list_workspace_actors(&client, &workspace_id, &params)
                        .with_context(|| format!("list actors in workspace `{workspace_id}`"))?;
                    println!("{}", serde_json::to_string_pretty(&data)?);
                }
                None => {
                    let data = list_actors(&client, &params).context("list actors")?;
                    println!("{}", serde_json::to_string_pretty(&data)?);
                }
            }
        }
        ActorCommand::Create {
            custom_id,
            display_name,
            actor_type,
            description,
            tags,
            metadata,
        } => {
            let data = create_actor(
                &client,
                &CreateActorRequest {
                    custom_id,
                    display_name,
                    actor_type: actor_type.map(ActorType::from),
                    description,
                    tags: tags.map(|list| list.0),
                    metadata,
                },
            )
            .context("create actor")?;
            println!("{}", serde_json::to_string_pretty(&data)?);
        }
        ActorCommand::Get { id, by_custom_id } => {
            let data = if by_custom_id {
                get_actor_by_custom_id(&client, &id).context("get actor by custom_id")?
            } else {
                get_actor(&client, &id).context("get actor")?
            };
            println!("{}", serde_json::to_string_pretty(&data)?);
        }
        ActorCommand::Update {
            id,
            display_name,
            description,
            tags,
            clear_tags,
            metadata,
        } => {
            let request = UpdateActorRequest {
                display_name,
                description,
                tags: resolve_tag_update(tags, clear_tags),
                metadata,
            };
            let data = update_actor(&client, &id, &request)
                .with_context(|| format!("update actor `{id}`"))?;
            println!("{}", serde_json::to_string_pretty(&data)?);
        }
        ActorCommand::Delete { id } => {
            delete_actor(&client, &id).with_context(|| format!("delete actor `{id}`"))?;
            println!("Deleted actor `{id}`");
        }
        ActorCommand::Bind(BindingArgs { workspace, actor }) => {
            let workspace = require_workspace(&paths, &runtime.profile, workspace)?;
            let data = bind_actor(&client, &workspace, &actor)
                .with_context(|| format!("bind actor `{actor}` to workspace `{workspace}`"))?;
            println!("{}", serde_json::to_string_pretty(&data)?);
        }
        ActorCommand::Unbind(BindingArgs { workspace, actor }) => {
            let workspace = require_workspace(&paths, &runtime.profile, workspace)?;
            unbind_actor(&client, &workspace, &actor)
                .with_context(|| format!("unbind actor `{actor}` from workspace `{workspace}`"))?;
            println!("Unbound actor `{actor}` from workspace `{workspace}`");
        }
    }

    Ok(())
}

/// Reject commands that cannot produce a meaningful request.
fn validate(command: &ActorCommand) -> Result<()> {
    if let ActorCommand::Update {
        display_name,
        description,
        tags,
        clear_tags,
        metadata,
        ..
    } = command
    {
        let request = UpdateActorRequest {
            display_name: display_name.clone(),
            description: description.clone(),
            tags: resolve_tag_update(tags.clone(), *clear_tags),
            metadata: metadata.clone(),
        };
        if request.is_empty() {
            bail!(
                "`actor update` requires at least one of --display-name, --description, --tags, --clear-tags, or --metadata"
            );
        }
    }
    Ok(())
}

/// Turn the two tag flags into the one field the API takes.
///
/// `--tags` sets the list, `--clear-tags` sends an empty one, and neither leaves
/// the stored tags alone. The distinction matters: an omitted `tags` is what
/// tells the server not to touch them, so "clear" cannot be expressed by simply
/// passing nothing.
fn resolve_tag_update(tags: Option<TagList>, clear: bool) -> Option<Vec<String>> {
    match tags {
        // clap rejects `--tags` alongside `--clear-tags`, so the two cannot
        // disagree by the time this runs.
        Some(list) => Some(list.0),
        None if clear => Some(Vec::new()),
        None => None,
    }
}

/// Parse a `--tags` value as a comma-separated list of labels.
///
/// Splitting on commas is lossless here rather than a convention: the API
/// rejects a tag containing a comma, so a comma can only ever be a separator.
/// Length and count limits are left to the server, which reports them precisely
/// (`tags[0] A tag must be 1 to 64 characters long`).
fn parse_tag_list(raw: &str) -> std::result::Result<TagList, String> {
    split_csv(raw).map(TagList)
}

/// Parse a `--metadata` value as a JSON object.
///
/// Rejects malformed JSON and valid JSON that is not an object, so an invalid
/// value never reaches the API.
fn parse_metadata_object(raw: &str) -> std::result::Result<Map<String, Value>, String> {
    let value: Value = serde_json::from_str(raw)
        .map_err(|err| format!("must be a JSON object: invalid JSON: {err}"))?;
    match value {
        Value::Object(map) => Ok(map),
        other => Err(format!("must be a JSON object, got {}", json_kind(&other))),
    }
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
    fn parse_metadata_object_accepts_an_object() {
        let map = parse_metadata_object(r#"{"tier":"premium","seats":3}"#).expect("valid object");
        assert_eq!(map["tier"], Value::String("premium".to_string()));
        assert_eq!(map["seats"], Value::Number(3.into()));
    }

    #[test]
    fn parse_metadata_object_accepts_an_empty_object() {
        assert!(
            parse_metadata_object("{}")
                .expect("empty object")
                .is_empty()
        );
    }

    #[test]
    fn parse_metadata_object_rejects_malformed_json() {
        let err = parse_metadata_object("not json").expect_err("malformed JSON must be rejected");
        assert!(err.contains("must be a JSON object"), "{err}");
        assert!(err.contains("invalid JSON"), "{err}");
    }

    #[test]
    fn parse_metadata_object_rejects_non_object_json() {
        for (raw, kind) in [
            ("[1,2]", "an array"),
            ("\"text\"", "a string"),
            ("42", "a number"),
            ("true", "a boolean"),
            ("null", "null"),
        ] {
            let err = parse_metadata_object(raw).expect_err("non-object JSON must be rejected");
            assert_eq!(err, format!("must be a JSON object, got {kind}"));
        }
    }

    /// An `actor update` carrying only the fields a test cares about.
    ///
    /// Spelled out rather than built with `..`: `ActorCommand` is an enum, and
    /// record-update syntax does not apply to variants.
    fn update_with(
        tags: Option<TagList>,
        clear_tags: bool,
        description: Option<&str>,
    ) -> ActorCommand {
        ActorCommand::Update {
            id: "act-1".to_string(),
            display_name: None,
            description: description.map(str::to_string),
            tags,
            clear_tags,
            metadata: None,
        }
    }

    #[test]
    fn validate_rejects_update_without_any_field() {
        let err = validate(&update_with(None, false, None))
            .expect_err("an empty update must be rejected");
        assert!(err.to_string().contains("at least one of"), "{err}");
        assert!(err.to_string().contains("--tags"), "{err}");
        assert!(err.to_string().contains("--clear-tags"), "{err}");
    }

    #[test]
    fn validate_accepts_update_with_one_field() {
        validate(&update_with(None, false, Some("updated"))).expect("a single field is enough");
    }

    #[test]
    fn validate_accepts_tags_as_the_only_field() {
        validate(&update_with(
            Some(TagList(vec!["vip".to_string()])),
            false,
            None,
        ))
        .expect("--tags alone is a real update");
    }

    #[test]
    fn validate_accepts_clear_tags_as_the_only_field() {
        // `--clear-tags` sends `"tags": []`, which is a change, so it must not
        // be mistaken for an empty request.
        validate(&update_with(None, true, None)).expect("--clear-tags alone is a real update");
    }

    #[test]
    fn resolve_tag_update_distinguishes_clear_from_untouched() {
        assert_eq!(resolve_tag_update(None, false), None, "neither flag");
        assert_eq!(
            resolve_tag_update(None, true),
            Some(Vec::new()),
            "--clear-tags must send an empty list, not nothing"
        );
        assert_eq!(
            resolve_tag_update(Some(TagList(vec!["a".to_string()])), false),
            Some(vec!["a".to_string()])
        );
    }

    #[test]
    fn parse_tag_list_splits_on_commas_and_trims() {
        assert_eq!(
            parse_tag_list(" vip , cn ").expect("valid list"),
            TagList(vec!["vip".to_string(), "cn".to_string()])
        );
    }

    #[test]
    fn parse_tag_list_keeps_a_single_tag() {
        assert_eq!(
            parse_tag_list("vip").expect("single tag"),
            TagList(vec!["vip".to_string()])
        );
    }

    #[test]
    fn parse_tag_list_rejects_empty_and_doubled_commas() {
        // `--tags ""` is far more likely a mistake than a request to clear, and
        // clearing has its own flag, so it must not silently mean either.
        assert!(parse_tag_list("").is_err());
        assert!(parse_tag_list("   ").is_err());
        let err = parse_tag_list("vip,,cn").expect_err("a doubled comma must be rejected");
        assert!(err.contains("empty entry"), "{err}");
    }

    #[test]
    fn parse_tag_list_preserves_case() {
        // The API matches tags exactly, so `VIP` must not be folded to `vip`.
        assert_eq!(
            parse_tag_list("VIP").expect("valid"),
            TagList(vec!["VIP".to_string()])
        );
    }

    #[test]
    fn actor_type_arg_maps_to_wire_values() {
        assert_eq!(ActorType::from(ActorTypeArg::Human).as_str(), "HUMAN");
        assert_eq!(
            ActorType::from(ActorTypeArg::Assistant).as_str(),
            "ASSISTANT"
        );
    }
}
