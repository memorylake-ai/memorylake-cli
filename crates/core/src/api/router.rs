//! MemoryRouter API (`/router`).
//!
//! MemoryRouter puts MemoryLake memory in front of OpenAI- and
//! Anthropic-compatible models: the request keeps the provider's own format,
//! and a `boundary_id` query parameter names the Boundary whose memories are
//! added to the prompt. Bodies and replies are the provider's JSON, so they
//! are handled here as [`Value`]s rather than modelled: the CLI forwards
//! whatever the provider accepts.
//!
//! The router sits under the same base URL as the rest of the API, in its own
//! `/router` section, and answers in the provider's shapes rather than the
//! MemoryLake envelope — see [`Client::router_json`].

use std::io::BufReader;

use reqwest::Method;
use reqwest::blocking::Response;
use serde_json::Value;

use crate::client::Client;
use crate::error::Result;
use crate::sse::EventStream;

/// Model catalog endpoint.
const MODELS_PATH: &str = "/router/v1/models";

/// `anthropic-version` sent with Messages requests, as the Anthropic SDKs do.
pub const ANTHROPIC_VERSION: &str = "2023-06-01";

/// One MemoryRouter inference endpoint.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Endpoint {
    /// OpenAI Chat Completions.
    ChatCompletions,
    /// OpenAI Responses.
    Responses,
    /// OpenAI Embeddings. Takes no Boundary and never streams.
    Embeddings,
    /// Anthropic Messages.
    Messages,
}

impl Endpoint {
    /// Path below the base URL.
    pub fn path(self) -> &'static str {
        match self {
            Self::ChatCompletions => "/router/openai/v1/chat/completions",
            Self::Responses => "/router/openai/v1/responses",
            Self::Embeddings => "/router/openai/v1/embeddings",
            Self::Messages => "/router/anthropic/v1/messages",
        }
    }

    /// Headers the provider's protocol expects besides authentication.
    fn headers(self) -> &'static [(&'static str, &'static str)] {
        match self {
            Self::Messages => &[("anthropic-version", ANTHROPIC_VERSION)],
            _ => &[],
        }
    }
}

/// A reply and the `X-Trace-ID` support asks for.
#[derive(Debug, Clone)]
pub struct Traced<T> {
    /// The reply.
    pub value: T,
    /// The response's `X-Trace-ID`, when it carried one.
    pub trace_id: Option<String>,
}

/// List the models MemoryRouter can call, with prices, as one array sorted by
/// name; each model's `provider_type` (`openai`, `anthropic`) says which base
/// URL reaches it.
///
/// The catalog answers in the MemoryLake format (`data.items`). An older
/// deployment that still groups models by protocol is flattened to the same
/// array.
pub fn list_models(client: &Client) -> Result<Traced<Value>> {
    let (value, trace_id) = client.router_json(Method::GET, MODELS_PATH, &[], &[], None)?;
    Ok(Traced {
        value: Value::Array(catalog_items(&value)),
        trace_id,
    })
}

/// The models of a catalog reply, in either shape.
fn catalog_items(reply: &Value) -> Vec<Value> {
    if let Some(items) = reply.pointer("/data/items").and_then(Value::as_array) {
        return items.clone();
    }
    let mut items: Vec<Value> = ["anthropic", "openai"]
        .iter()
        .filter_map(|group| reply.get(group).and_then(Value::as_array))
        .flatten()
        .cloned()
        .collect();
    items.sort_by(|a, b| a["name"].as_str().cmp(&b["name"].as_str()));
    items
}

/// Query string carrying `boundary`, when one is given.
fn boundary_query(boundary: Option<&str>) -> Vec<(&'static str, String)> {
    boundary
        .map(|id| vec![("boundary_id", id.to_string())])
        .unwrap_or_default()
}

/// Call `endpoint` with `body` and return the provider's reply.
///
/// With `boundary`, memories from that Boundary are added before the model
/// is called; without it the request goes to the model as is.
pub fn call(
    client: &Client,
    endpoint: Endpoint,
    boundary: Option<&str>,
    body: &Value,
) -> Result<Traced<Value>> {
    let (value, trace_id) = client.router_json(
        Method::POST,
        endpoint.path(),
        &boundary_query(boundary),
        endpoint.headers(),
        Some(body),
    )?;
    Ok(Traced { value, trace_id })
}

/// [`call`] for a body that asks to stream: the provider's server-sent
/// events as they arrive.
pub fn stream(
    client: &Client,
    endpoint: Endpoint,
    boundary: Option<&str>,
    body: &Value,
) -> Result<Traced<EventStream<BufReader<Response>>>> {
    let (value, trace_id) = client.router_event_stream(
        endpoint.path(),
        &boundary_query(boundary),
        endpoint.headers(),
        body,
    )?;
    Ok(Traced { value, trace_id })
}

#[cfg(test)]
mod catalog_tests {
    use super::catalog_items;
    use serde_json::json;

    #[test]
    fn items_come_from_data_items() {
        let reply = json!({"success":true,"data":{"items":[{"name":"a"},{"name":"b"}]}});
        assert_eq!(
            catalog_items(&reply),
            vec![json!({"name":"a"}), json!({"name":"b"})]
        );
    }

    #[test]
    fn the_older_grouped_reply_is_flattened_and_sorted() {
        let reply = json!({"anthropic":[{"name":"c"}],"openai":[{"name":"a"}]});
        assert_eq!(
            catalog_items(&reply),
            vec![json!({"name":"a"}), json!({"name":"c"})]
        );
    }
}
