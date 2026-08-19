//! Download a document's content
//! (`GET .../memories/documents/{document_id}/download`).

use std::io::Write;

use crate::client::{Client, Downloaded};
use crate::error::Result;

use super::path::document_path;

/// Stream a document's bytes into `writer`.
///
/// The endpoint answers `303` with a pre-signed storage URL rather than serving
/// the bytes itself; the redirect is followed automatically, and the API key is
/// not carried across to storage. That URL is short-lived and its query string
/// is a working credential, so it is never logged in full.
///
/// The content is streamed rather than buffered: documents are whole files, and
/// their size is the server's business, not this process's memory.
pub fn download_document<W: Write>(
    client: &Client,
    workspace_id: &str,
    project_id: &str,
    document_id: &str,
    writer: &mut W,
) -> Result<Downloaded> {
    let path = format!(
        "{}/download",
        document_path(workspace_id, project_id, document_id)
    );
    client.download_to(&path, writer)
}
