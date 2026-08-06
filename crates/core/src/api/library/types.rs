//! Shared Library resource types.

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};

/// Item-id alias that resolves to the workspace root.
///
/// Accepted anywhere an `item_id` is required, including as a parent when
/// creating items.
pub const ROOT_ALIAS: &str = "MY_SPACE";

/// `type` value the API reports for a folder.
///
/// Note the asymmetry with [`ITEM_TYPE_FOLDER`]: responses say `directory`
/// where requests say `folder`.
pub const ITEM_TYPE_DIRECTORY: &str = "directory";

/// `type` value the API reports for a file.
pub const ITEM_TYPE_FILE: &str = "file";

/// A file or folder in the Library.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Item {
    /// Fully-qualified drive URI.
    pub uri: String,
    /// Item identifier (`<space>:<inode>`), stable across renames.
    pub item_id: String,
    /// Display name within the parent folder.
    pub name: String,
    /// `file` or `directory`.
    ///
    /// Kept as a string rather than an enum so an unfamiliar server-side type
    /// degrades to a readable value instead of failing the whole response.
    #[serde(rename = "type")]
    pub item_type: String,
    /// Size in bytes. Folders report `0`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub size: Option<u64>,
    /// Content ETag. Absent on folders.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub etag: Option<String>,
    /// Parent folder's item id.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub parent_item_id: Option<String>,
    /// Creation timestamp (ISO 8601).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub created_at: Option<String>,
    /// Last update timestamp (ISO 8601).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub updated_at: Option<String>,
    /// Server- and caller-defined extended attributes.
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub x_attrs: BTreeMap<String, String>,
}

impl Item {
    /// Whether this item is a folder.
    pub fn is_directory(&self) -> bool {
        self.item_type == ITEM_TYPE_DIRECTORY
    }

    /// Whether this item is a file.
    pub fn is_file(&self) -> bool {
        self.item_type == ITEM_TYPE_FILE
    }
}

/// `item_type` value for creating a folder.
pub const ITEM_TYPE_FOLDER: &str = "folder";

/// What the server should do when the target name is already taken.
///
/// Sent as the `name_conflict_strategy` **body** field. The published overview
/// describes it as a query parameter; that form is silently ignored by the
/// server, which then falls back to [`NameConflictStrategy::Rename`].
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum NameConflictStrategy {
    /// Append a `_N` suffix and create a new item. Server default.
    Rename,
    /// Fail with `409 DRIVE_ITEM_CONFLICT`.
    Deny,
    /// Files only: replace the content in place, preserving `item_id`.
    Overwrite,
    /// Files only: delete and recreate, yielding a new `item_id`.
    Replace,
}

/// Identity of an item the server just created.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CreatedItem {
    /// Fully-qualified drive URI.
    pub uri: String,
    /// Item identifier assigned by the server.
    pub item_id: String,
    /// Final name. May differ from the requested name under
    /// [`NameConflictStrategy::Rename`], so always prefer this value.
    pub name: String,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn conflict_strategy_serializes_to_wire_values() {
        let json = serde_json::to_string(&NameConflictStrategy::Overwrite).unwrap();
        assert_eq!(json, r#""overwrite""#);
        let json = serde_json::to_string(&NameConflictStrategy::Rename).unwrap();
        assert_eq!(json, r#""rename""#);
    }

    #[test]
    fn folder_item_without_etag_deserializes() {
        // Folders come back with `size: 0` and no `etag` at all, contrary to
        // the published schema.
        let item: Item = serde_json::from_str(
            r#"{"uri":"drive://d/sc-a:inode-b","item_id":"sc-a:inode-b","name":"arena",
                "type":"directory","size":0,
                "parent_item_id":"sc-a:inode-r","created_at":"2026-02-25T09:54:53Z",
                "updated_at":"2026-02-25T09:54:53Z","x_attrs":{}}"#,
        )
        .expect("folder item deserializes");

        assert!(item.is_directory());
        assert!(!item.is_file());
        assert_eq!(item.size, Some(0));
        assert_eq!(item.etag, None);
        assert!(item.x_attrs.is_empty());
    }

    #[test]
    fn file_item_with_composite_etag_deserializes() {
        let item: Item = serde_json::from_str(
            r#"{"uri":"drive://d/sc-a:inode-c","item_id":"sc-a:inode-c","name":"report.pdf",
                "type":"file","size":6291456,"etag":"04b24ed91e487e950ad75cbff746b871-2",
                "parent_item_id":"sc-a:inode-r","x_attrs":{"k":"v"}}"#,
        )
        .expect("file item deserializes");

        assert!(item.is_file());
        assert_eq!(item.size, Some(6_291_456));
        // Multipart uploads yield a `<hash>-<part count>` composite ETag.
        assert_eq!(
            item.etag.as_deref(),
            Some("04b24ed91e487e950ad75cbff746b871-2")
        );
        assert_eq!(item.x_attrs.get("k").map(String::as_str), Some("v"));
    }
}
