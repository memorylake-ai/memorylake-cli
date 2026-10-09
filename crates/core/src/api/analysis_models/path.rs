//! URL construction for the analysis model endpoints.

use percent_encoding::{AsciiSet, NON_ALPHANUMERIC, utf8_percent_encode};

use crate::api::path::encode_segment;

/// Everything but RFC 3986 unreserved characters, so a query value can carry
/// no `&`, `=` or `#` of its own while `few_shot` still reads as itself.
const QUERY_VALUE_ENCODE_SET: &AsciiSet = &NON_ALPHANUMERIC
    .remove(b'-')
    .remove(b'_')
    .remove(b'.')
    .remove(b'~');

/// Collection path for the analysis models in `workspace_id`.
pub(super) fn models_path(workspace_id: &str) -> String {
    format!(
        "/api/v3/workspaces/{}/analysis-models",
        encode_segment(workspace_id)
    )
}

/// Path of the static template catalogue.
///
/// `templates` is a literal segment, so [`model_path`] cannot address a model
/// named `templates`. Server-assigned ids never are, but a caller-chosen
/// `custom_id` can be; the CLI rejects that combination before sending.
pub(super) fn templates_path(workspace_id: &str) -> String {
    format!("{}/templates", models_path(workspace_id))
}

/// Path for one analysis model.
///
/// `model_id` is the server-assigned id, or a caller-defined `custom_id` when
/// the request also sets `by_custom_id=true`.
pub(super) fn model_path(workspace_id: &str, model_id: &str) -> String {
    format!("{}/{}", models_path(workspace_id), encode_segment(model_id))
}

/// Collection path for one model's knowledge entries.
pub(super) fn entries_path(workspace_id: &str, model_id: &str) -> String {
    format!("{}/entries", model_path(workspace_id, model_id))
}

/// Path for one knowledge entry.
pub(super) fn entry_path(workspace_id: &str, model_id: &str, entry_id: &str) -> String {
    format!(
        "{}/{}",
        entries_path(workspace_id, model_id),
        encode_segment(entry_id)
    )
}

/// Path that excludes an entry from answering or restores it.
pub(super) fn entry_disable_path(workspace_id: &str, model_id: &str, entry_id: &str) -> String {
    format!("{}/disable", entry_path(workspace_id, model_id, entry_id))
}

/// `path` with an `entity_type` query parameter appended when one is given.
///
/// Only needed for DELETE: the client's delete helpers take a bare path, while
/// every other verb here passes query pairs through the client instead. The
/// value is fully percent-encoded so a kind name cannot inject further
/// parameters.
pub(super) fn with_entity_type(path: String, entity_type: Option<&str>) -> String {
    match entity_type {
        Some(kind) => format!(
            "{path}?entity_type={}",
            utf8_percent_encode(kind, QUERY_VALUE_ENCODE_SET)
        ),
        None => path,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn model_paths_are_relative_to_the_base_url() {
        assert_eq!(
            models_path("ws-1"),
            "/api/v3/workspaces/ws-1/analysis-models"
        );
        assert_eq!(
            templates_path("ws-1"),
            "/api/v3/workspaces/ws-1/analysis-models/templates"
        );
        assert_eq!(
            model_path("ws-1", "2810c96aaa8711f1b11d1e97653d06ca"),
            "/api/v3/workspaces/ws-1/analysis-models/2810c96aaa8711f1b11d1e97653d06ca"
        );
    }

    #[test]
    fn entry_paths_nest_under_their_model() {
        assert_eq!(
            entries_path("ws-1", "am-1"),
            "/api/v3/workspaces/ws-1/analysis-models/am-1/entries"
        );
        assert_eq!(
            entry_path("ws-1", "am-1", "e-1"),
            "/api/v3/workspaces/ws-1/analysis-models/am-1/entries/e-1"
        );
        assert_eq!(
            entry_disable_path("ws-1", "am-1", "e-1"),
            "/api/v3/workspaces/ws-1/analysis-models/am-1/entries/e-1/disable"
        );
    }

    #[test]
    fn every_segment_is_encoded_independently() {
        assert_eq!(
            entry_path("ws/1", "am?2", "e#3"),
            "/api/v3/workspaces/ws%2F1/analysis-models/am%3F2/entries/e%233"
        );
    }

    #[test]
    fn a_traversal_attempt_cannot_escape_its_segment() {
        assert_eq!(
            model_path("ws-1", "../templates"),
            "/api/v3/workspaces/ws-1/analysis-models/..%2Ftemplates"
        );
    }

    #[test]
    fn with_entity_type_appends_an_encoded_parameter() {
        assert_eq!(
            with_entity_type("/x".to_string(), Some("few_shot")),
            "/x?entity_type=few_shot"
        );
        assert_eq!(
            with_entity_type("/x".to_string(), Some("a&b=c")),
            "/x?entity_type=a%26b%3Dc"
        );
        assert_eq!(with_entity_type("/x".to_string(), None), "/x");
    }
}
