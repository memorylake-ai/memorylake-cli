//! Edit table and column annotations in one batch
//! (`PATCH /api/v3/workspaces/{workspace_id}/db-datasources/{datasource_id}/schema-metadata`).

use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::client::Client;
use crate::error::Result;

use super::path::db_datasource_child_path;

/// Most edits one request may carry.
pub const MAX_SCHEMA_METADATA_EDITS: usize = 200;

/// One annotation edit.
///
/// The wire shape is an object discriminated by `target`. Modeling it as an
/// enum means a column edit cannot be sent without its column, and a table
/// edit cannot carry a column-only field. Unknown keys are rejected when an
/// edit is read from JSON: in a hand-written batch, a misspelled `comment`
/// would otherwise turn into an edit that silently changes nothing.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "target", rename_all = "lowercase", deny_unknown_fields)]
pub enum SchemaMetadataEdit {
    /// Edit a table's annotation.
    Table {
        /// Table to edit.
        table_name: String,
        /// Schema the table is in. Optional while the datasource covers a
        /// single schema.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        schema_name: Option<String>,
        /// New annotation. `None` leaves it alone; an empty string clears it.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        comment: Option<String>,
    },
    /// Edit a column's annotation or its indexing flag.
    Column {
        /// Table the column belongs to.
        table_name: String,
        /// Column to edit.
        column_name: String,
        /// Schema the table is in. Optional while the datasource covers a
        /// single schema.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        schema_name: Option<String>,
        /// New annotation. `None` leaves it alone; an empty string clears it.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        comment: Option<String>,
        /// Whether to index the column's values for semantic matching. `None`
        /// leaves it as it is.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        embedding_enabled: Option<bool>,
    },
}

impl SchemaMetadataEdit {
    /// Whether this edit would change anything.
    ///
    /// An edit naming a table or column but carrying neither an annotation nor
    /// an indexing flag is legal on the wire, and almost certainly a mistake.
    pub fn changes_something(&self) -> bool {
        match self {
            Self::Table { comment, .. } => comment.is_some(),
            Self::Column {
                comment,
                embedding_enabled,
                ..
            } => comment.is_some() || embedding_enabled.is_some(),
        }
    }
}

/// Request body: the edits to apply, 1 to [`MAX_SCHEMA_METADATA_EDITS`].
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct UpdateSchemaMetadataRequest {
    /// Edits to apply.
    pub items: Vec<SchemaMetadataEdit>,
}

/// Apply annotation and indexing-flag edits to a datasource's tables and
/// columns in one batch.
///
/// Indexing changes take effect on the next
/// [build](super::build_db_datasource). The endpoint answers with an empty
/// envelope, which is validated and discarded.
pub fn update_db_datasource_schema_metadata(
    client: &Client,
    workspace_id: &str,
    datasource_id: &str,
    request: &UpdateSchemaMetadataRequest,
) -> Result<()> {
    client.patch_data::<Value, _>(
        &db_datasource_child_path(workspace_id, datasource_id, "schema-metadata"),
        request,
    )?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn edits_serialize_with_their_target() {
        let body = serde_json::to_value(UpdateSchemaMetadataRequest {
            items: vec![
                SchemaMetadataEdit::Table {
                    table_name: "orders".into(),
                    schema_name: None,
                    comment: Some("One row per order".into()),
                },
                SchemaMetadataEdit::Column {
                    table_name: "orders".into(),
                    column_name: "order_status".into(),
                    schema_name: Some("public".into()),
                    comment: Some(String::new()),
                    embedding_enabled: Some(true),
                },
            ],
        })
        .unwrap();
        assert_eq!(
            body,
            serde_json::json!({"items": [
                {"target": "table", "table_name": "orders", "comment": "One row per order"},
                {"target": "column", "table_name": "orders", "column_name": "order_status",
                 "schema_name": "public", "comment": "", "embedding_enabled": true}
            ]})
        );
    }

    #[test]
    fn unset_fields_are_left_out_so_they_stay_unchanged() {
        let body = serde_json::to_value(SchemaMetadataEdit::Column {
            table_name: "t".into(),
            column_name: "c".into(),
            schema_name: None,
            comment: None,
            embedding_enabled: Some(false),
        })
        .unwrap();
        assert_eq!(
            body,
            serde_json::json!({"target": "column", "table_name": "t", "column_name": "c",
                               "embedding_enabled": false})
        );
    }

    #[test]
    fn edits_parse_from_json() {
        let edits: Vec<SchemaMetadataEdit> = serde_json::from_str(
            r#"[{"target":"table","table_name":"orders","comment":"x"},
                {"target":"column","table_name":"orders","column_name":"id","embedding_enabled":false}]"#,
        )
        .expect("parse");
        assert_eq!(edits.len(), 2);
        assert!(edits.iter().all(SchemaMetadataEdit::changes_something));
    }

    #[test]
    fn malformed_edits_are_rejected() {
        for (json, why) in [
            (r#"{"table_name":"t","comment":"x"}"#, "missing target"),
            (r#"{"target":"schema","table_name":"t"}"#, "unknown target"),
            (
                r#"{"target":"column","table_name":"t","comment":"x"}"#,
                "column without column_name",
            ),
            (r#"{"target":"table","comment":"x"}"#, "missing table_name"),
            (
                r#"{"target":"table","table_name":"t","coment":"x"}"#,
                "misspelled field",
            ),
            (
                r#"{"target":"table","table_name":"t","embedding_enabled":true}"#,
                "column-only field on a table",
            ),
        ] {
            assert!(
                serde_json::from_str::<SchemaMetadataEdit>(json).is_err(),
                "{why}: {json}"
            );
        }
    }

    #[test]
    fn an_edit_with_nothing_to_change_is_recognized() {
        assert!(
            !SchemaMetadataEdit::Table {
                table_name: "t".into(),
                schema_name: None,
                comment: None,
            }
            .changes_something()
        );
        assert!(
            !SchemaMetadataEdit::Column {
                table_name: "t".into(),
                column_name: "c".into(),
                schema_name: Some("public".into()),
                comment: None,
                embedding_enabled: None,
            }
            .changes_something()
        );
    }
}
