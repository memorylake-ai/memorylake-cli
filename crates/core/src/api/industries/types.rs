//! Industry opendata resource types.

use serde::{Deserialize, Serialize};
use serde_json::{Map, Value};

/// One industry opendata collection a project can draw on.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct IndustryOpendata {
    /// Industry id, e.g. `financial/markets`. This is what a project's
    /// `industry_ids` takes.
    pub id: String,
    /// Display name.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,
    /// What the collection covers.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    /// Fields returned by the server that this client does not model.
    #[serde(flatten)]
    pub extra: Map<String, Value>,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn decodes_the_production_shape() {
        // Measured against production on 2026-10-09.
        let raw = r#"[{"id":"clinical/trials","name":"Clinical Trials","description":"Clinical trial records from ClinicalTrials.gov"}]"#;
        let items: Vec<IndustryOpendata> = serde_json::from_str(raw).expect("decode list");
        assert_eq!(items[0].id, "clinical/trials");
        assert_eq!(items[0].name.as_deref(), Some("Clinical Trials"));
        assert!(items[0].extra.is_empty());
    }

    #[test]
    fn keeps_unknown_fields() {
        let item: IndustryOpendata =
            serde_json::from_str(r#"{"id":"x/y","region":"US"}"#).expect("decode item");
        assert!(item.name.is_none());
        assert_eq!(serde_json::to_value(&item).unwrap()["region"], "US");
    }
}
