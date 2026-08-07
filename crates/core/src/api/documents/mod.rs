//! Project documents v3 API
//! (`/api/v3/workspaces/{id}/projects/{id}/memories/documents`).
//!
//! A document is a Library file that has been imported into a project and
//! indexed for semantic search and memory extraction. Files must already exist
//! in the Library — see [`library`](crate::api::library) for putting them
//! there.
//!
//! Importing is **asynchronous**. [`import_documents`] returns once the server
//! accepts the batch; each document then moves through `pending` / `running`
//! to `okay` or `error`, observable only through [`get_document`] or
//! [`list_documents`].

mod delete;
mod get;
mod import;
mod list;
mod path;
mod types;

pub use delete::{DeleteDocumentsRequest, delete_documents};
pub use get::get_document;
pub use import::{ImportDocumentsRequest, import_documents};
pub use list::{DocumentList, ListDocumentsParams, list_documents};
pub use types::{
    DOCUMENT_STATUS_ERROR, DOCUMENT_STATUS_OKAY, DOCUMENT_STATUS_PENDING, DOCUMENT_STATUS_RUNNING,
    Document, DocumentUsage, IMPORT_RESULT_DUPLICATE, IMPORT_RESULT_FAILED, IMPORT_RESULT_SUCCESS,
    ImportDetail, ImportOutcome, is_terminal_status,
};
