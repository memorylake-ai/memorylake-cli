//! A document's parsed graph
//! (`.../memories/documents/{document_id}/graph…` and `GET /api/v3/document-graph/schema`).
//!
//! Parsing turns a document into a tree of typed nodes (document root,
//! paragraphs, chunks, tables, figures, …). Which fields a node carries depends
//! on its type and is described by the platform-wide schema catalog, so the
//! responses are kept as JSON values rather than modelled per type. Only
//! accounts with the document graph enabled can call these endpoints; any other
//! account gets `403 FEATURE_NOT_ENABLED`.

use serde_json::Value;

use crate::client::Client;
use crate::error::Result;

use super::path::{graph_children_path, graph_node_path, graph_path};

/// Path of the platform-wide node type catalog.
const GRAPH_SCHEMA_PATH: &str = "/api/v3/document-graph/schema";

/// Options for [`list_graph_children`].
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct ListGraphChildrenParams {
    /// Most children per page (1–500; the server default is 100).
    pub count: Option<u32>,
    /// `continuation_token` of the previous page.
    pub continuation_token: Option<String>,
    /// `Some(false)` leaves out each child's own child counts (cheaper for leaf
    /// nodes such as chunks).
    pub with_edge_counts: Option<bool>,
    /// Comma-separated one-to-one link names of the children to return beside
    /// the page.
    pub expand: Option<String>,
}

/// Fetch the catalog of every node type a parsed document can hold.
pub fn get_graph_schema(client: &Client) -> Result<Value> {
    client.get_data(GRAPH_SCHEMA_PATH, &[])
}

/// Fetch a document's graph overview: its root node and per-type node counts.
///
/// `preview` (0–100) also returns the first N children of each of the root's
/// list relations.
pub fn get_document_graph(
    client: &Client,
    workspace_id: &str,
    project_id: &str,
    document_id: &str,
    preview: Option<u32>,
) -> Result<Value> {
    let mut query = Vec::new();
    if let Some(preview) = preview {
        query.push(("preview", preview.to_string()));
    }
    client.get_data(&graph_path(workspace_id, project_id, document_id), &query)
}

/// Fetch one node of a document's graph. `expand` names one-to-one links to
/// return beside it.
pub fn get_graph_node(
    client: &Client,
    workspace_id: &str,
    project_id: &str,
    document_id: &str,
    node_id: &str,
    expand: Option<&str>,
) -> Result<Value> {
    let mut query = Vec::new();
    if let Some(expand) = expand {
        query.push(("expand", expand.to_string()));
    }
    client.get_data(
        &graph_node_path(workspace_id, project_id, document_id, node_id),
        &query,
    )
}

/// Fetch one page of the children under one list relation of a node.
pub fn list_graph_children(
    client: &Client,
    workspace_id: &str,
    project_id: &str,
    document_id: &str,
    node_id: &str,
    edge: &str,
    params: &ListGraphChildrenParams,
) -> Result<Value> {
    let mut query = Vec::new();
    if let Some(count) = params.count {
        query.push(("count", count.to_string()));
    }
    if let Some(token) = &params.continuation_token {
        query.push(("continuation_token", token.clone()));
    }
    if let Some(with_edge_counts) = params.with_edge_counts {
        query.push(("with_edge_counts", with_edge_counts.to_string()));
    }
    if let Some(expand) = &params.expand {
        query.push(("expand", expand.clone()));
    }
    client.get_data(
        &graph_children_path(workspace_id, project_id, document_id, node_id, edge),
        &query,
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test_support::{json_ok, one_shot_server};

    #[test]
    fn overview_sends_preview_and_returns_the_data_verbatim() {
        let (base, server) = one_shot_server(json_ok(
            r#"{"success":true,"data":{"counts":{"chunk":2},"root":{"type":"txt","id":"txt-1"}}}"#,
        ));
        let client = Client::new(base, "sk-test").unwrap();
        let data = get_document_graph(&client, "ws-1", "proj-1", "doc-1", Some(5)).unwrap();
        assert_eq!(data["root"]["id"], "txt-1");
        assert_eq!(data["counts"]["chunk"], 2);

        let request = server.join().unwrap();
        assert!(
            request.head.starts_with(
                "GET /api/v3/workspaces/ws-1/projects/proj-1/memories/documents/doc-1/graph?preview=5 "
            ),
            "{}",
            request.head
        );
    }

    #[test]
    fn children_send_only_the_given_query() {
        let (base, server) =
            one_shot_server(json_ok(r#"{"success":true,"data":{"total":0,"items":[]}}"#));
        let client = Client::new(base, "sk-test").unwrap();
        list_graph_children(
            &client,
            "ws-1",
            "proj-1",
            "doc-1",
            "txt-1",
            "paragraphs",
            &ListGraphChildrenParams {
                count: Some(10),
                continuation_token: Some("tok".into()),
                with_edge_counts: Some(false),
                expand: None,
            },
        )
        .unwrap();
        let head = server.join().unwrap().head;
        assert!(
            head.contains("/graph/nodes/txt-1/children/paragraphs?"),
            "{head}"
        );
        for part in [
            "count=10",
            "continuation_token=tok",
            "with_edge_counts=false",
        ] {
            assert!(head.contains(part), "{head}");
        }
        assert!(!head.contains("expand="), "{head}");
    }

    #[test]
    fn node_and_schema_hit_their_paths() {
        let (base, server) = one_shot_server(json_ok(r#"{"success":true,"data":{"node":{}}}"#));
        let client = Client::new(base, "sk-test").unwrap();
        get_graph_node(
            &client,
            "ws-1",
            "proj-1",
            "doc-1",
            "figure-1",
            Some("summary"),
        )
        .unwrap();
        let head = server.join().unwrap().head;
        assert!(
            head.contains("/graph/nodes/figure-1?expand=summary "),
            "{head}"
        );

        let (base, server) = one_shot_server(json_ok(r#"{"success":true,"data":{"version":"v"}}"#));
        let client = Client::new(base, "sk-test").unwrap();
        let data = get_graph_schema(&client).unwrap();
        assert_eq!(data["version"], "v");
        assert!(
            server
                .join()
                .unwrap()
                .head
                .starts_with("GET /api/v3/document-graph/schema ")
        );
    }
}
