//! `memorylake actor` commands.

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

/// Workspace and actor pair shared by `bind` and `unbind`.
#[derive(Debug, Args)]
pub struct BindingArgs {
    /// Workspace id.
    #[arg(long)]
    pub workspace: String,
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
            workspace,
        } => {
            let params = ListActorsParams {
                page_size,
                continuation_token,
                actor_type: actor_type.map(ActorType::from),
                display_name_fuzzy,
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
            metadata,
        } => {
            let data = create_actor(
                &client,
                &CreateActorRequest {
                    custom_id,
                    display_name,
                    actor_type: actor_type.map(ActorType::from),
                    description,
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
            metadata,
        } => {
            let request = UpdateActorRequest {
                display_name,
                description,
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
            let data = bind_actor(&client, &workspace, &actor)
                .with_context(|| format!("bind actor `{actor}` to workspace `{workspace}`"))?;
            println!("{}", serde_json::to_string_pretty(&data)?);
        }
        ActorCommand::Unbind(BindingArgs { workspace, actor }) => {
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
        metadata,
        ..
    } = command
    {
        let request = UpdateActorRequest {
            display_name: display_name.clone(),
            description: description.clone(),
            metadata: metadata.clone(),
        };
        if request.is_empty() {
            bail!(
                "`actor update` requires at least one of --display-name, --description, or --metadata"
            );
        }
    }
    Ok(())
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

    #[test]
    fn validate_rejects_update_without_any_field() {
        let err = validate(&ActorCommand::Update {
            id: "act-1".to_string(),
            display_name: None,
            description: None,
            metadata: None,
        })
        .expect_err("an empty update must be rejected");
        assert!(err.to_string().contains("at least one of"), "{err}");
    }

    #[test]
    fn validate_accepts_update_with_one_field() {
        validate(&ActorCommand::Update {
            id: "act-1".to_string(),
            display_name: None,
            description: Some("updated".to_string()),
            metadata: None,
        })
        .expect("a single field is enough");
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
