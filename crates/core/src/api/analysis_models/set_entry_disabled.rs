//! Exclude or restore a knowledge entry
//! (`POST /api/v3/workspaces/{workspace_id}/analysis-models/{model_id}/entries/{entry_id}/disable`).

use serde::Serialize;
use serde::de::IgnoredAny;

use crate::client::Client;
use crate::error::Result;

use super::path::entry_disable_path;

/// Request body for the disable endpoint.
///
/// Despite the path, this is a setter rather than a toggle: `disabled: false`
/// puts the entry back into use.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct SetEntryDisabledRequest {
    /// `true` to exclude the entry from answering, `false` to restore it.
    pub disabled: bool,
    /// Kind of the entry. Optional, but some kinds cannot be located from the
    /// id alone.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub entity_type: Option<String>,
}

/// Take an entry out of use without deleting it, or put it back.
///
/// Success carries no payload (`ResponseWrapperVoid`). Whatever `data` holds is
/// decoded as [`IgnoredAny`] — an empty object, `null` and an absent key all
/// pass — while a failed envelope is still reported as an error.
pub fn set_entry_disabled(
    client: &Client,
    workspace_id: &str,
    model_id: &str,
    entry_id: &str,
    request: &SetEntryDisabledRequest,
) -> Result<()> {
    let _: IgnoredAny = client.post_data(
        &entry_disable_path(workspace_id, model_id, entry_id),
        request,
    )?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn restoring_sends_an_explicit_false() {
        let request = SetEntryDisabledRequest {
            disabled: false,
            entity_type: None,
        };
        assert_eq!(
            serde_json::to_string(&request).unwrap(),
            r#"{"disabled":false}"#
        );
    }

    #[test]
    fn entity_type_is_sent_when_known() {
        let request = SetEntryDisabledRequest {
            disabled: true,
            entity_type: Some("few_shot".into()),
        };
        assert_eq!(
            serde_json::to_string(&request).unwrap(),
            r#"{"disabled":true,"entity_type":"few_shot"}"#
        );
    }
}
