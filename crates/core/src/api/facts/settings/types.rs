//! Memory settings resource types.

use serde::{Deserialize, Serialize};
use serde_json::{Map, Value};

/// Longest fact instruction the API stores.
///
/// Counted the way the server counts — in UTF-16 code units, so a character
/// outside the Basic Multilingual Plane (most emoji) counts twice: 2000 CJK
/// characters are accepted, 1001 emoji are not (measured 2026-10-09).
pub const MAX_FACT_INSTRUCTION_LEN: usize = 2000;

/// The settings that govern how one scope's memory behaves.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct MemorySettings {
    /// What this memory is about and what it is not, in Markdown. It narrows
    /// what the server records here. An empty string means nothing has been
    /// set and the built-in default is in effect.
    pub fact_instruction: String,
    /// Fields not modeled above, preserved for output.
    #[serde(flatten)]
    pub extra: Map<String, Value>,
}

/// A partial settings update: only the fields present are changed.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize)]
pub struct UpdateMemorySettingsRequest {
    /// Replacement fact instruction, at most [`MAX_FACT_INSTRUCTION_LEN`].
    /// Replaces the previous value whole. `None` leaves it unchanged — the
    /// server also reads an explicit `null` that way (measured 2026-10-09) —
    /// and an empty string clears it, putting the built-in default back.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub fact_instruction: Option<String>,
}

/// What to steer a drafted fact instruction with. Every field is optional;
/// leaving them all unset drafts from the defaults, which is an ordinary use
/// and gives a slightly different draft on every call.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize)]
pub struct DraftFactInstructionRequest {
    /// What the draft should say, in any language. At most
    /// [`MAX_DRAFT_GUIDANCE_LEN`].
    #[serde(skip_serializing_if = "Option::is_none")]
    pub guidance: Option<String>,
    /// Language to draft in, as an **English language name** — "Simplified
    /// Chinese", "Japanese" — not a locale tag like `zh-CN`; the name is what
    /// produces a clean draft. Governs the headings as well as the sentences.
    /// The server drafts in English when unset. At most
    /// [`MAX_DRAFT_LANGUAGE_LEN`].
    #[serde(skip_serializing_if = "Option::is_none")]
    pub language: Option<String>,
    /// Let the draft read the facts already stored in this scope.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub use_existing_facts: Option<bool>,
    /// Let the draft read the names and summaries of the documents stored in
    /// this scope. Only projects hold documents; elsewhere it drafts from the
    /// defaults instead.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub use_documents: Option<bool>,
}

/// Longest `guidance` the draft endpoint accepts, in UTF-16 code units.
pub const MAX_DRAFT_GUIDANCE_LEN: usize = 2000;

/// Longest `language` the draft endpoint accepts, in UTF-16 code units.
pub const MAX_DRAFT_LANGUAGE_LEN: usize = 64;

/// A drafted fact instruction. It has **not** been saved.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct FactInstructionDraft {
    /// The drafted instruction, in Markdown.
    pub fact_instruction: String,
    /// Fields not modeled above, preserved for output.
    #[serde(flatten)]
    pub extra: Map<String, Value>,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn an_unset_instruction_is_left_out_but_an_empty_one_is_sent() {
        assert_eq!(
            serde_json::to_value(UpdateMemorySettingsRequest::default()).expect("serialize"),
            serde_json::json!({})
        );
        let clear = UpdateMemorySettingsRequest {
            fact_instruction: Some(String::new()),
        };
        assert_eq!(
            serde_json::to_value(&clear).expect("serialize"),
            serde_json::json!({"fact_instruction": ""})
        );
    }

    #[test]
    fn an_unsteered_draft_request_is_an_empty_object() {
        assert_eq!(
            serde_json::to_value(DraftFactInstructionRequest::default()).expect("serialize"),
            serde_json::json!({})
        );
    }

    #[test]
    fn every_draft_option_reaches_the_body() {
        let request = DraftFactInstructionRequest {
            guidance: Some("Only deadlines".into()),
            language: Some("Simplified Chinese".into()),
            use_existing_facts: Some(true),
            use_documents: Some(false),
        };
        assert_eq!(
            serde_json::to_value(&request).expect("serialize"),
            serde_json::json!({
                "guidance": "Only deadlines",
                "language": "Simplified Chinese",
                "use_existing_facts": true,
                "use_documents": false
            })
        );
    }

    #[test]
    fn settings_decode_and_keep_unmodeled_fields() {
        let settings: MemorySettings =
            serde_json::from_str(r#"{"fact_instruction": "", "future": true}"#).expect("decode");
        assert_eq!(settings.fact_instruction, "");
        assert_eq!(settings.extra.get("future"), Some(&serde_json::json!(true)));
    }
}
