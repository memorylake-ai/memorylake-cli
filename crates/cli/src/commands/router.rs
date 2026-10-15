//! `memorylake router` commands: call models through MemoryRouter.
//!
//! MemoryRouter speaks the OpenAI and Anthropic protocols, so each
//! subcommand builds the provider's own request body. A few flags cover the
//! common fields; `--body` takes a whole request for everything else, and the
//! flags are applied on top of it. Replies are printed as the provider's
//! JSON, or with `--text` / `--stream` as the reply text alone.

use std::io::Write;
use std::path::PathBuf;

use anyhow::{Context, Result, bail};
use clap::{Args, Subcommand, ValueEnum};
use memorylake_core::api::router::{Endpoint, Traced, call, list_models, stream};
use memorylake_core::sse::EventStream;
use serde_json::{Map, Value, json};

use super::input::{parse_non_empty, read_file_or_stdin};
use super::{api_client, print_json};

/// MemoryRouter subcommands.
#[derive(Debug, Subcommand)]
pub enum RouterCommand {
    /// List the models you can call and their prices, grouped by protocol.
    Models {
        /// Show only the models of one protocol.
        #[arg(long, value_enum)]
        protocol: Option<Protocol>,
    },
    /// OpenAI Chat Completions, with your memory attached.
    Chat(GenerateArgs),
    /// OpenAI Responses, with your memory attached.
    Responses(GenerateArgs),
    /// Anthropic Messages, with your memory attached.
    Messages(GenerateArgs),
    /// OpenAI Embeddings: one vector per input.
    Embed(EmbedArgs),
}

/// Protocol a model is called through.
#[derive(Debug, Clone, Copy, PartialEq, Eq, ValueEnum)]
pub enum Protocol {
    /// Models called through `router chat` and `router responses`.
    Openai,
    /// Models called through `router messages`.
    Anthropic,
}

/// Flags shared by the text-generating subcommands.
#[derive(Debug, Args)]
pub struct GenerateArgs {
    /// Message to send as the user. `-` reads it from standard input. It is
    /// appended after any conversation `--body` holds.
    #[arg(value_parser = parse_non_empty)]
    prompt: Option<String>,
    /// Model name, as `router models` lists it (e.g. `gpt-4o-mini`,
    /// `claude-haiku-4-5`).
    #[arg(long, value_parser = parse_non_empty)]
    model: Option<String>,
    /// Boundary whose memory the request uses. Without it the model is
    /// called without memory.
    #[arg(long, value_name = "ID", value_parser = parse_non_empty)]
    boundary: Option<String>,
    /// System prompt.
    #[arg(long)]
    system: Option<String>,
    /// Maximum tokens to generate. Messages defaults to 1024, which the
    /// Anthropic protocol requires.
    #[arg(long, value_parser = clap::value_parser!(u32).range(1..))]
    max_tokens: Option<u32>,
    /// Sampling temperature.
    #[arg(long)]
    temperature: Option<f64>,
    /// Whole request body in the provider's format, as a JSON file (`-` for
    /// standard input). The other flags are applied on top of it.
    #[arg(long, value_name = "FILE")]
    body: Option<PathBuf>,
    /// Stream the reply, printing its text as it arrives.
    #[arg(long)]
    stream: bool,
    /// Print only the reply text instead of the provider's JSON.
    #[arg(long, conflicts_with = "stream")]
    text: bool,
    /// With `--stream`, print each event's data as it arrives (one per
    /// line) instead of the reply text.
    #[arg(long, requires = "stream")]
    raw: bool,
}

/// Flags of `router embed`.
#[derive(Debug, Args)]
pub struct EmbedArgs {
    /// Texts to embed; one vector comes back per text.
    #[arg(value_parser = parse_non_empty)]
    input: Vec<String>,
    /// Embedding model name.
    #[arg(long, value_parser = parse_non_empty)]
    model: Option<String>,
    /// Length of each vector, for models that support it.
    #[arg(long, value_parser = clap::value_parser!(u32).range(1..))]
    dimensions: Option<u32>,
    /// Whole request body as a JSON file (`-` for standard input). The other
    /// flags are applied on top of it.
    #[arg(long, value_name = "FILE")]
    body: Option<PathBuf>,
}

/// Execute a `router` subcommand.
pub fn run(
    command: RouterCommand,
    profile: Option<String>,
    base_url: Option<String>,
) -> Result<()> {
    // Bodies are assembled first, so a malformed one costs no credentials.
    match command {
        RouterCommand::Models { protocol } => {
            let client = api_client(profile, base_url)?;
            let models = list_models(&client).context("list MemoryRouter models")?;
            print_trace(&models);
            match protocol {
                None => print_json(&models.value),
                Some(protocol) => {
                    let key = match protocol {
                        Protocol::Openai => "openai",
                        Protocol::Anthropic => "anthropic",
                    };
                    print_json(models.value.get(key).unwrap_or(&json!([])))
                }
            }
        }
        RouterCommand::Chat(args) => generate(Endpoint::ChatCompletions, args, profile, base_url),
        RouterCommand::Responses(args) => generate(Endpoint::Responses, args, profile, base_url),
        RouterCommand::Messages(args) => generate(Endpoint::Messages, args, profile, base_url),
        RouterCommand::Embed(args) => {
            let body = embed_body(args)?;
            let client = api_client(profile, base_url)?;
            let reply = call(&client, Endpoint::Embeddings, None, &Value::Object(body))
                .context("create embeddings")?;
            print_trace(&reply);
            print_json(&reply.value)
        }
    }
}

/// Run one of the text-generating subcommands.
fn generate(
    endpoint: Endpoint,
    args: GenerateArgs,
    profile: Option<String>,
    base_url: Option<String>,
) -> Result<()> {
    let boundary = args.boundary.clone();
    let (text, raw) = (args.text, args.raw);
    let body = generate_body(endpoint, args)?;
    let streaming = body.get("stream") == Some(&Value::Bool(true));
    let body = Value::Object(body);

    let client = api_client(profile, base_url)?;
    let action = format!("call {}", endpoint.path());
    if streaming {
        let events = stream(&client, endpoint, boundary.as_deref(), &body).context(action)?;
        print_trace(&events);
        return print_stream(endpoint, events.value, raw);
    }

    let reply = call(&client, endpoint, boundary.as_deref(), &body).context(action)?;
    print_trace(&reply);
    if text {
        println!("{}", reply_text(endpoint, &reply.value));
        Ok(())
    } else {
        print_json(&reply.value)
    }
}

/// Print a reply's trace id on standard error, the way `ETag`s are, so
/// standard output stays the reply alone.
fn print_trace<T>(reply: &Traced<T>) {
    if let Some(trace) = &reply.trace_id {
        eprintln!("X-Trace-ID: {trace}");
    }
}

/// Read `--body` into a JSON object, or start from an empty one.
fn load_body(path: Option<&PathBuf>) -> Result<Map<String, Value>> {
    let Some(path) = path else {
        return Ok(Map::new());
    };
    let text = read_file_or_stdin(path, "request body")?;
    match serde_json::from_str(&text)
        .with_context(|| format!("parse JSON from request body {}", path.display()))?
    {
        Value::Object(map) => Ok(map),
        _ => bail!("request body {} must hold a JSON object", path.display()),
    }
}

/// The prompt, read from standard input when it is `-`.
fn read_prompt(prompt: Option<String>) -> Result<Option<String>> {
    match prompt.as_deref() {
        Some("-") => {
            let text = read_file_or_stdin(std::path::Path::new("-"), "prompt")?;
            if text.trim().is_empty() {
                bail!("the prompt read from standard input is empty");
            }
            Ok(Some(text))
        }
        _ => Ok(prompt),
    }
}

/// Assemble a Chat Completions, Responses or Messages body from `--body` and
/// the flags, which win over it.
fn generate_body(endpoint: Endpoint, args: GenerateArgs) -> Result<Map<String, Value>> {
    let mut body = load_body(args.body.as_ref())?;
    let prompt = read_prompt(args.prompt)?;

    if let Some(model) = args.model {
        body.insert("model".into(), Value::String(model));
    }
    if let Some(temperature) = args.temperature {
        body.insert("temperature".into(), json!(temperature));
    }
    if args.stream {
        body.insert("stream".into(), Value::Bool(true));
    }
    // Each protocol names its output cap differently. Chat takes
    // `max_completion_tokens`, which the gpt-5 and o-series models require.
    let max_tokens_key = match endpoint {
        Endpoint::ChatCompletions => "max_completion_tokens",
        Endpoint::Responses => "max_output_tokens",
        _ => "max_tokens",
    };
    if let Some(max_tokens) = args.max_tokens {
        body.insert(max_tokens_key.into(), json!(max_tokens));
    } else if endpoint == Endpoint::Messages && !body.contains_key("max_tokens") {
        body.insert("max_tokens".into(), json!(1024));
    }

    match endpoint {
        Endpoint::Responses => {
            if let Some(system) = args.system {
                body.insert("instructions".into(), Value::String(system));
            }
            if let Some(prompt) = prompt {
                let input = match body.remove("input") {
                    None => Value::String(prompt),
                    Some(Value::String(earlier)) => {
                        json!([user_message(earlier), user_message(prompt)])
                    }
                    Some(Value::Array(mut items)) => {
                        items.push(user_message(prompt));
                        Value::Array(items)
                    }
                    Some(_) => bail!("`input` in the request body must be a string or an array"),
                };
                body.insert("input".into(), input);
            }
        }
        _ => {
            if let Some(system) = args.system {
                if endpoint == Endpoint::Messages {
                    body.insert("system".into(), Value::String(system));
                } else {
                    messages_mut(&mut body)?
                        .insert(0, json!({"role": "system", "content": system}));
                }
            }
            if let Some(prompt) = prompt {
                messages_mut(&mut body)?.push(user_message(prompt));
            }
        }
    }

    if !body.get("model").is_some_and(Value::is_string) {
        bail!("no model given; pass `--model` (see `memorylake router models`)");
    }
    let has_conversation = match endpoint {
        Endpoint::Responses => body.contains_key("input"),
        _ => body
            .get("messages")
            .and_then(Value::as_array)
            .is_some_and(|messages| !messages.is_empty()),
    };
    if !has_conversation {
        bail!("nothing to send; pass a prompt, or a conversation in `--body`");
    }
    Ok(body)
}

/// A user message in the shape both protocols accept.
fn user_message(text: String) -> Value {
    json!({"role": "user", "content": text})
}

/// The body's `messages` array, created when absent.
fn messages_mut(body: &mut Map<String, Value>) -> Result<&mut Vec<Value>> {
    match body
        .entry("messages")
        .or_insert_with(|| Value::Array(Vec::new()))
    {
        Value::Array(messages) => Ok(messages),
        _ => bail!("`messages` in the request body must be an array"),
    }
}

/// Assemble an Embeddings body from `--body` and the flags.
fn embed_body(args: EmbedArgs) -> Result<Map<String, Value>> {
    let mut body = load_body(args.body.as_ref())?;
    if let Some(model) = args.model {
        body.insert("model".into(), Value::String(model));
    }
    if let Some(dimensions) = args.dimensions {
        body.insert("dimensions".into(), json!(dimensions));
    }
    match args.input.len() {
        0 => {}
        1 => {
            let input = args.input.into_iter().next().unwrap_or_default();
            body.insert("input".into(), Value::String(input));
        }
        _ => {
            body.insert("input".into(), json!(args.input));
        }
    }
    if !body.get("model").is_some_and(Value::is_string) {
        bail!("no model given; pass `--model`");
    }
    if !body.contains_key("input") {
        bail!("nothing to embed; pass the texts, or `input` in `--body`");
    }
    Ok(body)
}

/// The reply text of a non-streamed reply, its parts joined.
fn reply_text(endpoint: Endpoint, reply: &Value) -> String {
    let parts: Vec<&str> = match endpoint {
        Endpoint::ChatCompletions => reply["choices"]
            .as_array()
            .into_iter()
            .flatten()
            .filter_map(|choice| choice["message"]["content"].as_str())
            .collect(),
        Endpoint::Responses => reply["output"]
            .as_array()
            .into_iter()
            .flatten()
            .flat_map(|item| item["content"].as_array().into_iter().flatten())
            .filter(|part| part["type"] == "output_text")
            .filter_map(|part| part["text"].as_str())
            .collect(),
        Endpoint::Messages => reply["content"]
            .as_array()
            .into_iter()
            .flatten()
            .filter(|block| block["type"] == "text")
            .filter_map(|block| block["text"].as_str())
            .collect(),
        Endpoint::Embeddings => Vec::new(),
    };
    parts.concat()
}

/// What one streamed event means for the reply.
#[derive(Debug, PartialEq)]
enum StreamStep {
    /// More reply text.
    Text(String),
    /// Nothing to print.
    Skip,
    /// The stream is over.
    Done,
}

/// Read one streamed event of `endpoint`'s protocol.
///
/// An error event — Anthropic's `error`, the Responses `error` and
/// `response.failed` — ends the command with its message.
fn stream_step(endpoint: Endpoint, data: &str) -> Result<StreamStep> {
    if data.trim() == "[DONE]" {
        return Ok(StreamStep::Done);
    }
    let event: Value = serde_json::from_str(data)
        .with_context(|| format!("the reply stream sent an event that is not JSON: {data}"))?;
    let kind = event["type"].as_str().unwrap_or_default();
    if kind == "error"
        || kind == "response.failed"
        || event.get("error").is_some_and(|e| !e.is_null())
    {
        let error = if kind == "response.failed" {
            &event["response"]["error"]
        } else {
            &event["error"]
        };
        let message = error["message"].as_str().unwrap_or(data);
        bail!("the model reported an error: {message}");
    }
    let text = match endpoint {
        Endpoint::ChatCompletions => event["choices"][0]["delta"]["content"].as_str(),
        Endpoint::Responses if kind == "response.output_text.delta" => event["delta"].as_str(),
        Endpoint::Messages if kind == "content_block_delta" => event["delta"]["text"].as_str(),
        Endpoint::Messages if kind == "message_stop" => return Ok(StreamStep::Done),
        _ => None,
    };
    Ok(text.map_or(StreamStep::Skip, |text| StreamStep::Text(text.to_string())))
}

/// Print a reply stream as it arrives: its text, or with `raw` each event's
/// data on a line of its own.
fn print_stream<R: std::io::BufRead>(
    endpoint: Endpoint,
    mut events: EventStream<R>,
    raw: bool,
) -> Result<()> {
    let stdout = std::io::stdout();
    let mut out = stdout.lock();
    let mut printed_text = false;
    while let Some(event) = events.next_event().context("read the reply stream")? {
        let Some(data) = event.data else { continue };
        if raw {
            writeln!(out, "{data}")?;
            out.flush()?;
            if data.trim() == "[DONE]" {
                break;
            }
            continue;
        }
        match stream_step(endpoint, &data)? {
            StreamStep::Text(text) => {
                write!(out, "{text}")?;
                out.flush()?;
                printed_text = true;
            }
            StreamStep::Skip => {}
            StreamStep::Done => break,
        }
    }
    if printed_text {
        writeln!(out)?;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn args(prompt: Option<&str>) -> GenerateArgs {
        GenerateArgs {
            prompt: prompt.map(str::to_string),
            model: Some("m".into()),
            boundary: None,
            system: None,
            max_tokens: None,
            temperature: None,
            body: None,
            stream: false,
            text: false,
            raw: false,
        }
    }

    #[test]
    fn chat_body_puts_the_system_prompt_first_and_the_prompt_last() {
        let mut a = args(Some("hi"));
        a.system = Some("be brief".into());
        a.max_tokens = Some(64);
        let body = generate_body(Endpoint::ChatCompletions, a).unwrap();
        assert_eq!(
            Value::Object(body),
            json!({
                "model": "m",
                "max_completion_tokens": 64,
                "messages": [
                    {"role": "system", "content": "be brief"},
                    {"role": "user", "content": "hi"}
                ]
            })
        );
    }

    #[test]
    fn messages_body_defaults_max_tokens_and_uses_the_system_field() {
        let mut a = args(Some("hi"));
        a.system = Some("be brief".into());
        let body = generate_body(Endpoint::Messages, a).unwrap();
        assert_eq!(body["max_tokens"], 1024);
        assert_eq!(body["system"], "be brief");
        assert_eq!(body["messages"], json!([{"role": "user", "content": "hi"}]));
    }

    #[test]
    fn responses_body_sends_a_lone_prompt_as_a_string() {
        let mut a = args(Some("hi"));
        a.system = Some("be brief".into());
        a.stream = true;
        let body = generate_body(Endpoint::Responses, a).unwrap();
        assert_eq!(body["input"], "hi");
        assert_eq!(body["instructions"], "be brief");
        assert_eq!(body["stream"], true);
    }

    #[test]
    fn a_body_without_model_or_conversation_is_refused() {
        let mut a = args(Some("hi"));
        a.model = None;
        let err = generate_body(Endpoint::ChatCompletions, a).unwrap_err();
        assert!(err.to_string().contains("--model"), "{err}");

        let err = generate_body(Endpoint::Messages, args(None)).unwrap_err();
        assert!(err.to_string().contains("nothing to send"), "{err}");
    }

    #[test]
    fn embed_body_sends_one_text_as_a_string_and_several_as_an_array() {
        let one = embed_body(EmbedArgs {
            input: vec!["a".into()],
            model: Some("e".into()),
            dimensions: Some(8),
            body: None,
        })
        .unwrap();
        assert_eq!(
            Value::Object(one),
            json!({"model": "e", "dimensions": 8, "input": "a"})
        );

        let two = embed_body(EmbedArgs {
            input: vec!["a".into(), "b".into()],
            model: Some("e".into()),
            dimensions: None,
            body: None,
        })
        .unwrap();
        assert_eq!(two["input"], json!(["a", "b"]));
    }

    #[test]
    fn reply_text_reads_each_protocol() {
        let chat = json!({"choices": [{"message": {"role": "assistant", "content": "Lisbon"}}]});
        assert_eq!(reply_text(Endpoint::ChatCompletions, &chat), "Lisbon");
        let responses = json!({"output": [
            {"type": "reasoning", "content": []},
            {"type": "message", "content": [{"type": "output_text", "text": "Lisbon"}]}
        ]});
        assert_eq!(reply_text(Endpoint::Responses, &responses), "Lisbon");
        let messages =
            json!({"content": [{"type": "text", "text": "Lis"}, {"type": "text", "text": "bon"}]});
        assert_eq!(reply_text(Endpoint::Messages, &messages), "Lisbon");
    }

    #[test]
    fn stream_steps_follow_each_protocol() {
        let text = |s: &str| StreamStep::Text(s.into());
        assert_eq!(
            stream_step(
                Endpoint::ChatCompletions,
                r#"{"choices":[{"delta":{"content":"Li"}}]}"#
            )
            .unwrap(),
            text("Li")
        );
        assert_eq!(
            stream_step(Endpoint::ChatCompletions, "[DONE]").unwrap(),
            StreamStep::Done
        );
        assert_eq!(
            stream_step(
                Endpoint::Responses,
                r#"{"type":"response.output_text.delta","delta":"Li"}"#
            )
            .unwrap(),
            text("Li")
        );
        assert_eq!(
            stream_step(
                Endpoint::Responses,
                r#"{"type":"response.created","response":{}}"#
            )
            .unwrap(),
            StreamStep::Skip
        );
        assert_eq!(
            stream_step(
                Endpoint::Messages,
                r#"{"type":"content_block_delta","index":0,"delta":{"type":"text_delta","text":"Li"}}"#
            )
            .unwrap(),
            text("Li")
        );
        assert_eq!(
            stream_step(Endpoint::Messages, r#"{"type":"message_stop"}"#).unwrap(),
            StreamStep::Done
        );
    }

    #[test]
    fn stream_error_events_end_the_command() {
        let err = stream_step(
            Endpoint::Messages,
            r#"{"type":"error","error":{"type":"overloaded_error","message":"Overloaded"}}"#,
        )
        .unwrap_err();
        assert!(err.to_string().contains("Overloaded"), "{err}");
        let err = stream_step(
            Endpoint::Responses,
            r#"{"type":"response.failed","response":{"error":{"message":"boom"}}}"#,
        )
        .unwrap_err();
        assert!(err.to_string().contains("boom"), "{err}");
    }
}
