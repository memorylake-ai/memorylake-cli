//! `--compact` output: only the fields an agent needs to decide what to open.
//!
//! The full payloads carry usage counters, creator ids, drive URIs, summaries
//! and every matched block. That's useful to a person, but it eats an agent's
//! context one listing at a time. The compact shapes keep the ids that the next
//! command takes (`project document get` / `download` need a project and a
//! document id), a name to show the user, and the status that says whether a
//! document can be used yet.
//!
//! Fields the server did not send are left out instead of printed as `null`.

use memorylake_core::api::documents::Document;
use memorylake_core::api::projects::Project;
use memorylake_core::api::search::{SearchDocument, SearchFact, SearchResults};
use serde_json::{Map, Value, json};

/// Longest matched-text snippet kept per search hit, in characters.
pub const SNIPPET_MAX_CHARS: usize = 200;

/// Help text shared by every `--compact` flag.
pub const COMPACT_HELP: &str = "Print only the fields needed to pick a result (ids, name, project, status), not the full payload";

/// Insert `value` under `key` unless it is `None`.
fn put<T: Into<Value>>(map: &mut Map<String, Value>, key: &str, value: Option<T>) {
    if let Some(value) = value {
        map.insert(key.to_string(), value.into());
    }
}

/// A project: its id, name, and the `custom_id` callers map it by.
pub fn project(project: &Project) -> Value {
    let mut map = Map::new();
    map.insert("id".into(), project.id.clone().into());
    map.insert("name".into(), project.name.clone().into());
    put(&mut map, "custom_id", project.custom_id.clone());
    put(&mut map, "updated_at", project.updated_at.clone());
    Value::Object(map)
}

/// A document of `project_id`: what `get` / `download` take, and its status.
///
/// `error` is kept only for a document that failed, since that's the one time
/// it says something.
pub fn document(project_id: &str, document: &Document) -> Value {
    let mut map = Map::new();
    map.insert("id".into(), document.id.clone().into());
    map.insert("name".into(), document.name.clone().into());
    map.insert("project_id".into(), project_id.into());
    map.insert("status".into(), document.status.clone().into());
    put(&mut map, "document_type", document.document_type.clone());
    put(&mut map, "created_at", document.created_at.clone());
    if document.is_error() {
        put(&mut map, "error", document.error.clone());
    }
    Value::Object(map)
}

/// Fold every run of whitespace (line breaks included) into one space, then
/// cut to at most [`SNIPPET_MAX_CHARS`] characters, marking a cut with `…`.
/// Counts characters, not bytes, so CJK text is never split mid-character.
fn snippet(text: &str) -> String {
    let text = text.split_whitespace().collect::<Vec<_>>().join(" ");
    match text.char_indices().nth(SNIPPET_MAX_CHARS) {
        Some((cut, _)) => format!("{}…", &text[..cut]),
        None => text,
    }
}

fn search_document(hit: &SearchDocument) -> Value {
    let mut map = Map::new();
    put(&mut map, "rank", hit.rank);
    map.insert("document_id".into(), hit.document_id.clone().into());
    put(&mut map, "project_id", hit.project_id.clone());
    put(
        &mut map,
        "document_name",
        hit.document_name.clone().or_else(|| hit.file_name.clone()),
    );
    let first = hit.items.iter().flat_map(|item| item.snippets()).next();
    put(&mut map, "snippet", first.map(snippet));
    Value::Object(map)
}

fn search_fact(hit: &SearchFact) -> Value {
    let mut map = Map::new();
    put(&mut map, "rank", hit.rank);
    map.insert("id".into(), hit.id.clone().into());
    put(&mut map, "fact", hit.fact.clone());
    put(&mut map, "updated_at", hit.updated_at.clone());
    Value::Object(map)
}

/// Search results: one entry per hit, the first matched snippet in place of
/// every block. Databases pass through unchanged: their shape is not typed and
/// they rarely show up.
pub fn search(results: &SearchResults) -> Value {
    json!({
        "documents": results.documents.iter().map(search_document).collect::<Vec<_>>(),
        "facts": results.facts.iter().map(search_fact).collect::<Vec<_>>(),
        "databases": results.databases,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn snippet_keeps_short_text_and_cuts_long_text_on_a_character() {
        assert_eq!(snippet("  short  "), "short");
        assert_eq!(snippet("line one\n\n  line\ttwo"), "line one line two");
        let long = "零".repeat(SNIPPET_MAX_CHARS + 5);
        let cut = snippet(&long);
        assert_eq!(cut.chars().count(), SNIPPET_MAX_CHARS + 1);
        assert!(cut.ends_with('…'));
        let exact = "a".repeat(SNIPPET_MAX_CHARS);
        assert_eq!(snippet(&exact), exact);
    }

    fn doc(status: &str) -> Document {
        serde_json::from_value(json!({
            "id": "doc-1",
            "name": "a.pdf",
            "status": status,
            "error": {"message": "bad pdf"},
            "usage": {"files_process_tokens": 10},
            "document_type": "drive_file",
            "created_at": "2026-11-13T10:11:58Z"
        }))
        .expect("document")
    }

    #[test]
    fn document_keeps_the_ids_and_status_and_drops_the_rest() {
        assert_eq!(
            document("proj-1", &doc("okay")),
            json!({
                "id": "doc-1",
                "name": "a.pdf",
                "project_id": "proj-1",
                "status": "okay",
                "document_type": "drive_file",
                "created_at": "2026-11-13T10:11:58Z"
            })
        );
    }

    #[test]
    fn document_keeps_the_error_of_a_failed_document() {
        assert_eq!(
            document("proj-1", &doc("error"))["error"],
            json!({"message": "bad pdf"})
        );
    }

    #[test]
    fn search_keeps_one_snippet_per_document_and_drops_summaries() {
        let results: SearchResults = serde_json::from_value(json!({
            "documents": [{
                "rank": 1,
                "document_id": "doc-1",
                "project_id": "proj-1",
                "document_name": "a.pdf",
                "document_summary": "long summary",
                "items": [
                    {"type": "paragraph", "highlight": {"chunks": [{"text": "first"}, {"text": "second"}]}},
                    {"type": "figure", "text": "third"}
                ]
            }],
            "facts": [{"rank": 2, "id": "f-1", "fact": "likes tea", "score": 0.5, "metadata": {}}],
            "databases": []
        }))
        .expect("results");
        assert_eq!(
            search(&results),
            json!({
                "documents": [{
                    "rank": 1,
                    "document_id": "doc-1",
                    "project_id": "proj-1",
                    "document_name": "a.pdf",
                    "snippet": "first"
                }],
                "facts": [{"rank": 2, "id": "f-1", "fact": "likes tea"}],
                "databases": []
            })
        );
    }
}
