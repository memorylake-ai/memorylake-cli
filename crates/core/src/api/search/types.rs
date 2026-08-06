//! Search request filters and result types.

use std::fmt;

use serde::{Deserialize, Serialize};

/// Wire value for [`MemoryType::Document`].
pub const MEMORY_TYPE_DOCUMENT: &str = "document";

/// Wire value for [`MemoryType::Fact`].
pub const MEMORY_TYPE_FACT: &str = "fact";

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
}

impl MemoryType {
    /// Every accepted value, in the order help text and errors should list them.
    pub const ALL: [Self; 2] = [Self::Document, Self::Fact];

    /// Wire representation of this memory type.
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Document => MEMORY_TYPE_DOCUMENT,
            Self::Fact => MEMORY_TYPE_FACT,
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

/// One matched span inside a document.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct DocumentItem {
    /// Matched text.
    #[serde(default)]
    pub text: Option<String>,
    /// Location of the match within the source document.
    #[serde(default)]
    pub range: Option<String>,
}

/// A document that matched the query, with its matching spans.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SearchDocument {
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
    /// Matching spans. Absent means none were returned.
    #[serde(default)]
    pub items: Vec<DocumentItem>,
}

/// A fact that matched the query.
///
/// `score` is `f64`, so this type is only [`PartialEq`] — unlike the other
/// resource types in this crate, it cannot derive [`Eq`].
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SearchFact {
    /// Server-assigned fact id.
    pub id: String,
    /// The fact text.
    #[serde(default)]
    pub fact: Option<String>,
    /// Relevance score. The API does not guarantee it is always present.
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
/// The collections are independent result sets rather than one ranked list, and
/// none is paginated. The server returns only the collections the request asked
/// for — filtering on `memory_types` drops the others entirely — so a missing
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
    /// The API returns this alongside `documents` and `facts` but does not
    /// document it, and no sample payload has been observed with entries in it.
    /// Rather than guess a schema — or drop server data on the floor — the
    /// entries pass through untyped so they still reach the caller's output.
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
    fn the_undocumented_databases_collection_is_preserved() {
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
