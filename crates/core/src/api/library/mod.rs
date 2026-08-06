//! Library API (`/api/v1/drives/*`).
//!
//! The Library is MemoryLake's unified file system. Items are addressed by
//! opaque `item_id` (`<space>:<inode>`); the alias [`ROOT_ALIAS`] stands in for
//! the workspace root.
//!
//! Note that this family sits at `/api/v1`, unlike the workspace endpoints at
//! `/api/v3`.

mod create;
mod delete;
mod get;
mod list;
mod paths;
mod types;
mod upload;

pub use create::{CreateFileRequest, CreateFolderRequest, PartETag, create_file, create_folder};
pub use delete::delete_item;
pub use get::get_item;
pub use list::{ItemList, ListChildrenParams, list_children};
pub use types::{
    CreatedItem, ITEM_TYPE_DIRECTORY, ITEM_TYPE_FILE, ITEM_TYPE_FOLDER, Item, NameConflictStrategy,
    ROOT_ALIAS,
};
pub use upload::{PartItem, UploadFileRequest, UploadSession, create_upload_session, upload_file};
