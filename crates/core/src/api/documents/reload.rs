//! Process a failed document again
//! (`POST .../projects/{project_id}/memories/documents/{document_id}/reload`).

use crate::client::Client;
use crate::error::Result;

use super::path::reload_document_path;

/// Queue a document whose status is `error` for processing again.
///
/// The document keeps its id, its status returns to `pending`, and the
/// outcome arrives asynchronously, like an import. The retry is not charged
/// again. A document in any other status is refused with
/// `400 STATE_NOT_READY`. Success carries no payload (`{"success": true}`,
/// measured 2026-10-09).
pub fn reload_document(
    client: &Client,
    workspace_id: &str,
    project_id: &str,
    document_id: &str,
) -> Result<()> {
    client.post_empty(
        &reload_document_path(workspace_id, project_id, document_id),
        &serde_json::json!({}),
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test_support::{json_ok, one_shot_server};

    #[test]
    fn reload_posts_to_the_verb_and_accepts_an_envelope_without_data() {
        let (base, server) = one_shot_server(json_ok(r#"{"success":true}"#));
        let client = Client::new(base, "sk-test").unwrap();
        reload_document(&client, "ws-1", "proj-1", "doc-1").expect("reload");

        let request = server.join().unwrap();
        assert!(
            request.head.starts_with(
                "POST /api/v3/workspaces/ws-1/projects/proj-1/memories/documents/doc-1/reload "
            ),
            "{}",
            request.head
        );
    }
}
