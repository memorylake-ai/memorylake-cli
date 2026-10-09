//! Create a folder or finalize an uploaded file (`POST /api/v1/drives/items`).

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};

use crate::client::Client;
use crate::error::Result;

use super::paths::ITEMS_PATH;
use super::types::{CreatedItem, ITEM_TYPE_FILE, ITEM_TYPE_FOLDER, NameConflictStrategy};

/// Create a folder.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CreateFolderRequest {
    /// Parent folder id, or [`ROOT_ALIAS`](super::ROOT_ALIAS).
    pub parent_item_id: String,
    /// Requested name.
    pub name: String,
    /// Behavior on name collision. `None` uses the server default (`rename`).
    /// Folders accept only `rename` and `deny`.
    pub name_conflict_strategy: Option<NameConflictStrategy>,
    /// Extended attributes to create the folder with. `None` sends none.
    pub x_attrs: Option<BTreeMap<String, String>>,
}

/// Finalize a chunked upload as a file item.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CreateFileRequest {
    /// Parent folder id, or [`ROOT_ALIAS`](super::ROOT_ALIAS).
    pub parent_item_id: String,
    /// Requested name.
    pub name: String,
    /// Upload session that holds the stored bytes.
    pub upload_id: String,
    /// One entry per uploaded part, in the server's numbering.
    pub part_etags: Vec<PartETag>,
    /// Behavior on name collision. `None` uses the server default (`rename`).
    pub name_conflict_strategy: Option<NameConflictStrategy>,
    /// Extended attributes to create the file with. `None` sends none.
    pub x_attrs: Option<BTreeMap<String, String>>,
}

/// A part number paired with the ETag storage returned for it.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PartETag {
    /// 1-based part number, as assigned by the upload session.
    pub number: u32,
    /// ETag exactly as the storage backend returned it, quotes included.
    pub etag: String,
}

#[derive(Debug, Serialize)]
struct CreateFolderBody<'a> {
    item_type: &'static str,
    parent_item_id: &'a str,
    name: &'a str,
    #[serde(skip_serializing_if = "Option::is_none")]
    name_conflict_strategy: Option<NameConflictStrategy>,
}

#[derive(Debug, Serialize)]
struct CreateFileBody<'a> {
    item_type: &'static str,
    parent_item_id: &'a str,
    name: &'a str,
    from: UploadSource<'a>,
    #[serde(skip_serializing_if = "Option::is_none")]
    name_conflict_strategy: Option<NameConflictStrategy>,
}

#[derive(Debug, Serialize)]
struct UploadSource<'a> {
    upload_id: &'a str,
    part_etags: &'a [PartETag],
}

/// A create body plus the optional `x_attrs` both item types accept.
///
/// A wrapper rather than one more field on each body, so the bodies keep the
/// shape their own tests pin.
#[derive(Debug, Serialize)]
struct WithXattrs<'a, B> {
    #[serde(flatten)]
    body: B,
    #[serde(skip_serializing_if = "Option::is_none")]
    x_attrs: Option<&'a BTreeMap<String, String>>,
}

/// Create an empty folder.
pub fn create_folder(client: &Client, request: &CreateFolderRequest) -> Result<CreatedItem> {
    client.post_data(
        ITEMS_PATH,
        &WithXattrs {
            body: CreateFolderBody {
                item_type: ITEM_TYPE_FOLDER,
                parent_item_id: &request.parent_item_id,
                name: &request.name,
                name_conflict_strategy: request.name_conflict_strategy,
            },
            x_attrs: request.x_attrs.as_ref(),
        },
    )
}

/// Register previously uploaded bytes as a file item.
///
/// Until this call succeeds the uploaded parts are not visible in the Library.
pub fn create_file(client: &Client, request: &CreateFileRequest) -> Result<CreatedItem> {
    client.post_data(
        ITEMS_PATH,
        &WithXattrs {
            body: CreateFileBody {
                item_type: ITEM_TYPE_FILE,
                parent_item_id: &request.parent_item_id,
                name: &request.name,
                from: UploadSource {
                    upload_id: &request.upload_id,
                    part_etags: &request.part_etags,
                },
                name_conflict_strategy: request.name_conflict_strategy,
            },
            x_attrs: request.x_attrs.as_ref(),
        },
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn x_attrs_are_appended_only_when_given() {
        let folder = || CreateFolderBody {
            item_type: ITEM_TYPE_FOLDER,
            parent_item_id: "MY_SPACE",
            name: "docs",
            name_conflict_strategy: None,
        };
        assert_eq!(
            serde_json::to_string(&WithXattrs {
                body: folder(),
                x_attrs: None,
            })
            .unwrap(),
            r#"{"item_type":"folder","parent_item_id":"MY_SPACE","name":"docs"}"#
        );

        let attrs = BTreeMap::from([("team".to_string(), "core".to_string())]);
        assert_eq!(
            serde_json::to_string(&WithXattrs {
                body: folder(),
                x_attrs: Some(&attrs),
            })
            .unwrap(),
            r#"{"item_type":"folder","parent_item_id":"MY_SPACE","name":"docs","x_attrs":{"team":"core"}}"#
        );
    }

    #[test]
    fn folder_body_omits_strategy_when_unset() {
        let body = CreateFolderBody {
            item_type: ITEM_TYPE_FOLDER,
            parent_item_id: "MY_SPACE",
            name: "docs",
            name_conflict_strategy: None,
        };
        assert_eq!(
            serde_json::to_string(&body).unwrap(),
            r#"{"item_type":"folder","parent_item_id":"MY_SPACE","name":"docs"}"#
        );
    }

    #[test]
    fn folder_body_carries_strategy_in_the_body_not_the_query() {
        // Verified against the live API: a `name_conflict_strategy` query
        // parameter is ignored and the server renames instead of denying.
        let body = CreateFolderBody {
            item_type: ITEM_TYPE_FOLDER,
            parent_item_id: "MY_SPACE",
            name: "docs",
            name_conflict_strategy: Some(NameConflictStrategy::Deny),
        };
        assert_eq!(
            serde_json::to_string(&body).unwrap(),
            r#"{"item_type":"folder","parent_item_id":"MY_SPACE","name":"docs","name_conflict_strategy":"deny"}"#
        );
    }

    #[test]
    fn file_body_nests_upload_source_under_from() {
        let etags = vec![
            PartETag {
                number: 1,
                etag: "\"aaa\"".into(),
            },
            PartETag {
                number: 2,
                etag: "\"bbb\"".into(),
            },
        ];
        let body = CreateFileBody {
            item_type: ITEM_TYPE_FILE,
            parent_item_id: "sc-a:inode-b",
            name: "big.bin",
            from: UploadSource {
                upload_id: "u-1",
                part_etags: &etags,
            },
            name_conflict_strategy: Some(NameConflictStrategy::Overwrite),
        };
        assert_eq!(
            serde_json::to_string(&body).unwrap(),
            r#"{"item_type":"file","parent_item_id":"sc-a:inode-b","name":"big.bin","from":{"upload_id":"u-1","part_etags":[{"number":1,"etag":"\"aaa\""},{"number":2,"etag":"\"bbb\""}]},"name_conflict_strategy":"overwrite"}"#
        );
    }
}
