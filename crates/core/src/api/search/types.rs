//! Search request filters and result types.

use std::fmt;

use serde::{Deserialize, Serialize};
use serde_json::{Map, Value};

/// Wire value for [`MemoryType::Document`].
pub const MEMORY_TYPE_DOCUMENT: &str = "document";

/// Wire value for [`MemoryType::Fact`].
pub const MEMORY_TYPE_FACT: &str = "fact";

/// Wire value for [`MemoryType::Database`].
pub const MEMORY_TYPE_DATABASE: &str = "database";

/// A memory source type accepted by the `memory_types` search filter.
///
/// Unlike [`crate::api::actors::ActorType`], this is strict: it only ever
/// travels to the server as a filter the caller typed, never back as data, so
/// an unrecognized value is a user mistake rather than a server-side addition
/// this build should tolerate.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(into = "String")]
pub enum MemoryType {
    /// Ingested documents.
    Document,
    /// Extracted facts.
    Fact,
    /// Structured/tabular sources.
    Database,
}

impl MemoryType {
    /// Every accepted value, in the order help text and errors should list them.
    pub const ALL: [Self; 3] = [Self::Document, Self::Fact, Self::Database];

    /// Wire representation of this memory type.
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Document => MEMORY_TYPE_DOCUMENT,
            Self::Fact => MEMORY_TYPE_FACT,
            Self::Database => MEMORY_TYPE_DATABASE,
        }
    }

    /// Parse a wire value, returning `None` when it is not recognized.
    ///
    /// Matching is exact: the API's values are lowercase, and accepting other
    /// spellings here would hide a typo until the request came back rejected.
    pub fn from_wire(raw: &str) -> Option<Self> {
        match raw {
            MEMORY_TYPE_DOCUMENT => Some(Self::Document),
            MEMORY_TYPE_FACT => Some(Self::Fact),
            MEMORY_TYPE_DATABASE => Some(Self::Database),
            _ => None,
        }
    }
}

impl fmt::Display for MemoryType {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

impl From<MemoryType> for String {
    fn from(value: MemoryType) -> Self {
        value.as_str().to_string()
    }
}

/// One matched block inside a document.
///
/// The API sends one of several block shapes, told apart by `type`. A figure
/// carries its matched `text` and `range` at the top level; a paragraph, table
/// or NL2SQL block carries them in `highlight.chunks` instead. Use
/// [`Self::snippets`] to read the matched text without caring which.
///
/// Only the fields shared by the shapes are typed. Everything else (`caption`,
/// `table_id`, `title`, ...) is kept in `extra`, so it still reaches the
/// caller's output rather than being dropped.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct DocumentItem {
    /// Block kind, e.g. `paragraph`, `subtable`, `figure`, `nl2sql`. Kept as
    /// sent so a kind this build does not know still decodes.
    #[serde(rename = "type", default, skip_serializing_if = "Option::is_none")]
    pub kind: Option<String>,
    /// Matched text, for blocks that carry it at the top level (figures).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub text: Option<String>,
    /// Location of the match, for blocks that carry it at the top level.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub range: Option<String>,
    /// Matched chunks, for paragraph, table and NL2SQL blocks.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub highlight: Option<Highlight>,
    /// Block-specific fields this build does not model.
    #[serde(flatten)]
    pub extra: Map<String, Value>,
}

impl DocumentItem {
    /// The matched text of this block: the top-level `text` when present,
    /// otherwise each non-empty `highlight.chunks[].text` in order.
    pub fn snippets(&self) -> Vec<&str> {
        if let Some(text) = self.text.as_deref().filter(|text| !text.is_empty()) {
            return vec![text];
        }
        self.highlight
            .iter()
            .flat_map(|highlight| &highlight.chunks)
            .filter_map(|chunk| chunk.text.as_deref())
            .filter(|text| !text.is_empty())
            .collect()
    }
}

/// The matched part of a paragraph, table or NL2SQL block.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Highlight {
    /// Matched text spans.
    #[serde(default)]
    pub chunks: Vec<HighlightChunk>,
    /// Shape-specific fields (`inner_tables`, `instruction`, ...).
    #[serde(flatten)]
    pub extra: Map<String, Value>,
}

/// One matched text span inside a [`Highlight`].
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct HighlightChunk {
    /// Chunk id.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub id: Option<String>,
    /// Matched text.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub text: Option<String>,
    /// Location of the match within the source document.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub range: Option<String>,
}

/// A document that matched the query, with its matching spans.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SearchDocument {
    /// 1-based position in the one relevance order shared by `documents`,
    /// `databases` and `facts`. See [`SearchResults`].
    #[serde(default)]
    pub rank: Option<u32>,
    /// Server-assigned document id.
    pub document_id: String,
    /// Display name.
    #[serde(default)]
    pub document_name: Option<String>,
    /// Original file name.
    #[serde(default)]
    pub file_name: Option<String>,
    /// Summary of the whole document.
    #[serde(default)]
    pub document_summary: Option<String>,
    /// How the document entered MemoryLake.
    #[serde(default)]
    pub source_type: Option<String>,
    /// Sheet name for spreadsheet sources; documented as nullable.
    #[serde(default)]
    pub sheet_name: Option<String>,
    /// Matching blocks. Absent means none were returned.
    #[serde(default)]
    pub items: Vec<DocumentItem>,
}

/// A fact that matched the query.
///
/// `score` is `f64`, so this type is only [`PartialEq`] — unlike the other
/// resource types in this crate, it cannot derive [`Eq`].
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SearchFact {
    /// 1-based position in the one relevance order shared by `documents`,
    /// `databases` and `facts`. See [`SearchResults`].
    #[serde(default)]
    pub rank: Option<u32>,
    /// Server-assigned fact id.
    pub id: String,
    /// The fact text.
    #[serde(default)]
    pub fact: Option<String>,
    /// Ordering score, equal to `1/rank`.
    ///
    /// It encodes position only, not relevance magnitude, and is not
    /// comparable across requests -- never use it as a threshold. Prefer
    /// `rank`.
    #[serde(default)]
    pub score: Option<f64>,
    /// Caller-defined metadata stored with the fact.
    #[serde(default)]
    pub metadata: Option<serde_json::Value>,
    /// Creation timestamp (ISO 8601).
    #[serde(default)]
    pub created_at: Option<String>,
    /// Last update timestamp (ISO 8601).
    #[serde(default)]
    pub updated_at: Option<String>,
}

/// The `data` payload of a memory search.
///
/// The three collections are views of ONE ranked list, split by memory type:
/// every entry carries a `rank` unique across all of them, and merging by
/// `rank` restores the order the search computed, best first. None is
/// paginated. The server returns only the collections the request asked for —
/// filtering on `memory_types` drops the others entirely — so a missing
/// collection decodes as empty and "no matches" and "key absent" look the same
/// to callers.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct SearchResults {
    /// Documents that matched.
    #[serde(default)]
    pub documents: Vec<SearchDocument>,
    /// Facts that matched.
    #[serde(default)]
    pub facts: Vec<SearchFact>,
    /// Structured/tabular sources that matched.
    ///
    /// Documented with the same shape as `documents`, but no live payload has
    /// been observed with entries in it, so the entries pass through untyped:
    /// a mismatch with the documented shape must not fail the whole search.
    #[serde(default)]
    pub databases: Vec<serde_json::Value>,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn memory_type_round_trips_its_wire_values() {
        for value in MemoryType::ALL {
            assert_eq!(MemoryType::from_wire(value.as_str()), Some(value));
        }
        assert_eq!(
            serde_json::to_string(&MemoryType::Document).expect("serialize"),
            r#""document""#
        );
        assert_eq!(
            serde_json::to_string(&MemoryType::Fact).expect("serialize"),
            r#""fact""#
        );
        assert_eq!(
            serde_json::to_string(&MemoryType::Database).expect("serialize"),
            r#""database""#
        );
    }

    #[test]
    fn memory_type_rejects_anything_else() {
        for raw in ["Document", "DOCUMENT", "facts", "memo", "", " document"] {
            assert_eq!(MemoryType::from_wire(raw), None, "should reject {raw:?}");
        }
    }

    #[test]
    fn documented_payload_deserializes() {
        let results: SearchResults = serde_json::from_str(
            r#"{
                "documents": [{
                    "document_id": "doc-1",
                    "document_name": "Q4 report",
                    "file_name": "q4.xlsx",
                    "document_summary": "Quarterly figures",
                    "source_type": "upload",
                    "sheet_name": "Revenue",
                    "items": [{"text": "Revenue was 1.2M", "range": "A1:C9"}]
                }],
                "facts": [{
                    "id": "fact-1",
                    "fact": "Q4 revenue was 1.2M",
                    "score": 0.87,
                    "metadata": {"source": "q4.xlsx"},
                    "created_at": "2026-01-01T00:00:00Z",
                    "updated_at": "2026-01-02T00:00:00Z"
                }]
            }"#,
        )
        .expect("deserialize documented payload");

        assert_eq!(results.documents.len(), 1);
        assert_eq!(results.documents[0].document_id, "doc-1");
        assert_eq!(results.documents[0].sheet_name.as_deref(), Some("Revenue"));
        assert_eq!(results.documents[0].items.len(), 1);
        assert_eq!(
            results.documents[0].items[0].text.as_deref(),
            Some("Revenue was 1.2M")
        );
        assert_eq!(results.facts[0].score, Some(0.87));
    }

    #[test]
    fn typed_block_shapes_decode_and_expose_their_snippets() {
        // The documented `items[]` union: a figure carries `text` at the top
        // level, the other shapes put it in `highlight.chunks`.
        let results: SearchResults = serde_json::from_str(
            r#"{
                "documents": [{
                    "rank": 2,
                    "document_id": "doc-1",
                    "items": [
                        {"type": "paragraph", "paragraph_id": "p-1",
                         "highlight": {"chunks": [
                             {"id": "c-1", "text": "first", "range": "p.1"},
                             {"id": "c-2", "text": "second"}
                         ]}},
                        {"type": "subtable", "title": "Revenue", "table_id": "t-1",
                         "highlight": {"chunks": [{"text": "row"}], "inner_tables": []}},
                        {"type": "figure", "text": "a chart", "range": "p.3",
                         "caption": "Figure 1"}
                    ]
                }],
                "facts": [{"rank": 1, "id": "fact-1", "score": 1.0}]
            }"#,
        )
        .expect("deserialize block shapes");

        let doc = &results.documents[0];
        assert_eq!(doc.rank, Some(2));
        assert_eq!(results.facts[0].rank, Some(1));
        assert_eq!(doc.items[0].kind.as_deref(), Some("paragraph"));
        assert_eq!(doc.items[0].snippets(), vec!["first", "second"]);
        assert_eq!(doc.items[1].snippets(), vec!["row"]);
        assert_eq!(doc.items[2].snippets(), vec!["a chart"]);

        // Fields this build does not model must survive into the output.
        let rendered = serde_json::to_value(&results).expect("serialize");
        let items = &rendered["documents"][0]["items"];
        assert_eq!(items[0]["paragraph_id"], "p-1");
        assert_eq!(items[0]["highlight"]["chunks"][0]["range"], "p.1");
        assert_eq!(items[1]["table_id"], "t-1");
        assert_eq!(items[1]["highlight"]["inner_tables"], serde_json::json!([]));
        assert_eq!(items[2]["caption"], "Figure 1");
        // A shape without top-level text must not print a stray `"text": null`.
        assert!(items[0].get("text").is_none(), "{items}");
    }

    #[test]
    fn every_uncertain_field_may_be_absent() {
        // The docs do not promise these are populated, so none may be required.
        let cases = [
            "{}",
            r#"{"documents":[]}"#,
            r#"{"facts":[]}"#,
            r#"{"documents":[{"document_id":"doc-1"}]}"#,
            r#"{"documents":[{"document_id":"doc-1","sheet_name":null,"items":[]}]}"#,
            r#"{"facts":[{"id":"fact-1"}]}"#,
            r#"{"facts":[{"id":"fact-1","score":null,"metadata":null}]}"#,
        ];
        for raw in cases {
            serde_json::from_str::<SearchResults>(raw)
                .unwrap_or_else(|err| panic!("should decode {raw}: {err}"));
        }
    }

    #[test]
    fn absent_collections_decode_as_empty() {
        let results: SearchResults = serde_json::from_str("{}").expect("decode");
        assert_eq!(results, SearchResults::default());
        assert!(results.documents.is_empty());
        assert!(results.facts.is_empty());
        assert!(results.databases.is_empty());
    }

    #[test]
    fn the_databases_collection_is_preserved() {
        // Observed live: a query without `memory_types` answers with
        // `["databases", "documents", "facts"]`. Dropping it would silently
        // discard server data from the command's output.
        let results: SearchResults = serde_json::from_str(
            r#"{"documents":[],"facts":[],"databases":[{"table":"sales","rows":3}]}"#,
        )
        .expect("decode payload with databases");

        assert_eq!(results.databases.len(), 1);
        assert_eq!(results.databases[0]["table"], "sales");

        // It must also survive back out to the printed JSON.
        let rendered = serde_json::to_value(&results).expect("serialize");
        assert_eq!(rendered["databases"][0]["rows"], 3);
    }

    #[test]
    fn a_filtered_response_returning_one_collection_decodes() {
        // Observed live: `memory_types:["document"]` answers with `documents`
        // only; the other keys are absent rather than empty.
        let results: SearchResults =
            serde_json::from_str(r#"{"documents":[{"document_id":"doc-1"}]}"#).expect("decode");
        assert_eq!(results.documents.len(), 1);
        assert!(results.facts.is_empty());
        assert!(results.databases.is_empty());
    }

    #[test]
    fn unknown_fields_do_not_break_decoding() {
        // The server may add fields; that must never fail an existing build.
        let results: SearchResults = serde_json::from_str(
            r#"{"documents":[{"document_id":"doc-1","future_field":1}],
                "facts":[{"id":"f-1","future_field":{"a":2}}],
                "total_hits":3}"#,
        )
        .expect("decode payload with unknown fields");
        assert_eq!(results.documents.len(), 1);
        assert_eq!(results.facts.len(), 1);
    }
}
