//! Argument parsing for `conversation` commands.
//!
//! Two things need turning from flags into request fields: message content,
//! which reaches the API as an array of typed blocks, and `key=value`
//! metadata.

use std::path::Path;

use anyhow::{Context, Result, bail};
use memorylake_core::api::conversations::{BLOCK_TYPES, ContentBlock, Metadata, text_block};
use serde_json::Value;

/// Parse a `key=value` metadata pair.
///
/// Splits on the first `=` so values may contain more of them. The key must be
/// non-empty; the value may be, since clearing a metadata entry is meaningful.
pub fn parse_metadata_pair(raw: &str) -> std::result::Result<(String, String), String> {
    let Some((key, value)) = raw.split_once('=') else {
        return Err(format!("expected `key=value`, found `{raw}`"));
    };
    let key = key.trim();
    if key.is_empty() {
        return Err(format!("metadata key must not be empty in `{raw}`"));
    }
    Ok((key.to_string(), value.to_string()))
}

/// Collect metadata pairs into the map the API expects.
///
/// A repeated key keeps the last value, matching how a later flag overrides an
/// earlier one everywhere else in the CLI. Returns `None` when no pair was
/// given so the field stays absent from the request body.
pub fn collect_metadata(pairs: Vec<(String, String)>) -> Option<Metadata> {
    if pairs.is_empty() {
        return None;
    }
    Some(pairs.into_iter().collect())
}

/// Build message content from the mutually exclusive input flags.
///
/// `--text` covers the common case; `--content-json` / `--content-file` carry
/// the block types the CLI does not model (`IMAGE`, `TOOL_USE`, …). Mixing
/// `--text` with a JSON source is rejected rather than concatenated: the
/// resulting block order would depend on a rule nobody wrote down.
pub fn build_content(
    texts: Vec<String>,
    content_json: Option<String>,
    content_file: Option<&Path>,
) -> Result<Vec<ContentBlock>> {
    let json_source = match (content_json, content_file) {
        (Some(_), Some(_)) => {
            bail!("pass --content-json or --content-file, not both")
        }
        (Some(inline), None) => Some((inline, "--content-json".to_string())),
        (None, Some(path)) => {
            let text = std::fs::read_to_string(path)
                .with_context(|| format!("read content file {}", path.display()))?;
            Some((text, format!("content file {}", path.display())))
        }
        (None, None) => None,
    };

    match (texts.is_empty(), json_source) {
        (false, Some((_, source))) => {
            bail!("--text and {source} both set message content; pass only one")
        }
        (true, None) => bail!(
            "message content is required: pass --text <TEXT>, \
             or --content-json / --content-file for non-text blocks"
        ),
        (false, None) => Ok(texts.into_iter().map(text_block).collect()),
        (true, Some((json, source))) => parse_content_blocks(&json, &source),
    }
}

/// Parse and validate a JSON array of content blocks.
///
/// Validation is deliberately shallow: every block must be an object carrying
/// a `block_type` string, because that field discriminates the block and a
/// missing one is always rejected server-side. Per-type fields are not
/// checked, and a `block_type` outside [`BLOCK_TYPES`] passes through with the
/// documented list only quoted in errors — the server stays the authority on
/// what it accepts.
fn parse_content_blocks(json: &str, source: &str) -> Result<Vec<ContentBlock>> {
    let value: Value =
        serde_json::from_str(json).with_context(|| format!("parse JSON from {source}"))?;

    let Value::Array(items) = value else {
        bail!(
            "{source} must hold a JSON array of content blocks, found {}\n\
             example: [{{\"block_type\": \"TEXT\", \"text\": \"hello\"}}]",
            json_kind(&value)
        );
    };
    if items.is_empty() {
        bail!("{source} holds an empty array; a message needs at least one content block");
    }

    let mut blocks = Vec::with_capacity(items.len());
    for (index, item) in items.into_iter().enumerate() {
        let Value::Object(block) = item else {
            bail!(
                "{source}: block {index} must be a JSON object, found {}",
                json_kind(&item)
            );
        };
        match block.get("block_type") {
            Some(Value::String(kind)) if !kind.trim().is_empty() => {}
            Some(Value::String(_)) | None => bail!(
                "{source}: block {index} is missing a non-empty `block_type` (one of {})",
                BLOCK_TYPES.join(", ")
            ),
            Some(other) => bail!(
                "{source}: block {index} has a `block_type` of {}, expected a string (one of {})",
                json_kind(other),
                BLOCK_TYPES.join(", ")
            ),
        };
        blocks.push(block);
    }
    Ok(blocks)
}

fn json_kind(value: &Value) -> &'static str {
    match value {
        Value::Null => "null",
        Value::Bool(_) => "a boolean",
        Value::Number(_) => "a number",
        Value::String(_) => "a string",
        Value::Array(_) => "an array",
        Value::Object(_) => "an object",
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;
    use std::path::PathBuf;
    use std::sync::atomic::{AtomicU64, Ordering};

    fn temp_json(contents: &str) -> PathBuf {
        static NEXT: AtomicU64 = AtomicU64::new(0);
        let path = std::env::temp_dir().join(format!(
            "memorylake-conversation-content-{}-{}.json",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        ));
        let mut file = std::fs::File::create(&path).expect("create temp content file");
        file.write_all(contents.as_bytes())
            .expect("write content file");
        path
    }

    fn err_of(result: Result<Vec<ContentBlock>>) -> String {
        format!("{:#}", result.expect_err("expected a rejection"))
    }

    #[test]
    fn texts_become_one_text_block_each_in_order() {
        let blocks = build_content(vec!["first".into(), "second".into()], None, None)
            .expect("build content");
        assert_eq!(blocks.len(), 2);
        assert_eq!(blocks[0]["block_type"], "TEXT");
        assert_eq!(blocks[0]["text"], "first");
        assert_eq!(blocks[1]["text"], "second");
    }

    #[test]
    fn inline_json_blocks_pass_through_with_every_field() {
        let blocks = build_content(
            Vec::new(),
            Some(r#"[{"block_type":"TOOL_USE","tool_call_id":"c1","tool_name":"search","arguments":{"q":"x"}}]"#.into()),
            None,
        )
        .expect("build content");
        assert_eq!(blocks[0]["tool_name"], "search");
        assert_eq!(blocks[0]["arguments"]["q"], "x");
    }

    #[test]
    fn a_block_type_the_cli_does_not_know_is_still_forwarded() {
        // The server is the authority on accepted block types; rejecting one
        // here would break the CLI the day the API grows a seventh.
        let blocks = build_content(
            Vec::new(),
            Some(r#"[{"block_type":"VIDEO","uri":"s3://x"}]"#.into()),
            None,
        )
        .expect("build content");
        assert_eq!(blocks[0]["block_type"], "VIDEO");
    }

    #[test]
    fn content_files_are_read_and_parsed() {
        let path = temp_json(r#"[{"block_type":"TEXT","text":"from file"}]"#);
        let blocks = build_content(Vec::new(), None, Some(&path)).expect("build content");
        assert_eq!(blocks[0]["text"], "from file");
        let _ = std::fs::remove_file(&path);
    }

    #[test]
    fn missing_content_names_every_way_to_supply_it() {
        let err = err_of(build_content(Vec::new(), None, None));
        assert!(err.contains("--text"), "got: {err}");
        assert!(err.contains("--content-json"), "got: {err}");
        assert!(err.contains("--content-file"), "got: {err}");
    }

    #[test]
    fn text_and_json_together_are_rejected_rather_than_concatenated() {
        let err = err_of(build_content(
            vec!["hi".into()],
            Some(r#"[{"block_type":"TEXT","text":"x"}]"#.into()),
            None,
        ));
        assert!(err.contains("only one"), "got: {err}");
    }

    #[test]
    fn two_json_sources_together_are_rejected() {
        let path = temp_json("[]");
        let err = err_of(build_content(Vec::new(), Some("[]".into()), Some(&path)));
        assert!(err.contains("not both"), "got: {err}");
        let _ = std::fs::remove_file(&path);
    }

    #[test]
    fn malformed_json_names_its_source() {
        let err = err_of(build_content(Vec::new(), Some("{not json".into()), None));
        assert!(err.contains("parse JSON from --content-json"), "got: {err}");

        let path = temp_json("{not json");
        let err = err_of(build_content(Vec::new(), None, Some(&path)));
        assert!(err.contains("parse JSON from content file"), "got: {err}");
        let _ = std::fs::remove_file(&path);
    }

    #[test]
    fn a_missing_content_file_is_reported_as_such() {
        let path = std::env::temp_dir().join("memorylake-conversation-absent.json");
        let err = err_of(build_content(Vec::new(), None, Some(&path)));
        assert!(err.contains("read content file"), "got: {err}");
    }

    #[test]
    fn a_non_array_top_level_is_rejected_with_an_example() {
        let err = err_of(build_content(
            Vec::new(),
            Some(r#"{"block_type":"TEXT","text":"x"}"#.into()),
            None,
        ));
        assert!(err.contains("array of content blocks"), "got: {err}");
        assert!(err.contains("an object"), "got: {err}");
        assert!(err.contains("example:"), "got: {err}");
    }

    #[test]
    fn an_empty_array_is_rejected() {
        let err = err_of(build_content(Vec::new(), Some("[]".into()), None));
        assert!(err.contains("at least one content block"), "got: {err}");
    }

    #[test]
    fn a_block_without_a_block_type_is_rejected_before_the_request() {
        let err = err_of(build_content(
            Vec::new(),
            Some(r#"[{"block_type":"TEXT","text":"ok"},{"text":"no type"}]"#.into()),
            None,
        ));
        assert!(err.contains("block 1"), "got: {err}");
        assert!(err.contains("block_type"), "got: {err}");
        assert!(
            err.contains("TOOL_RESULT"),
            "the error lists known types: {err}"
        );
    }

    #[test]
    fn a_non_string_block_type_is_rejected() {
        let err = err_of(build_content(
            Vec::new(),
            Some(r#"[{"block_type": 7}]"#.into()),
            None,
        ));
        assert!(err.contains("expected a string"), "got: {err}");
    }

    #[test]
    fn a_non_object_block_is_rejected() {
        let err = err_of(build_content(Vec::new(), Some(r#"["TEXT"]"#.into()), None));
        assert!(err.contains("block 0 must be a JSON object"), "got: {err}");
    }

    #[test]
    fn metadata_splits_on_the_first_equals_only() {
        assert_eq!(
            parse_metadata_pair("url=https://x/?a=b").expect("parse"),
            ("url".to_string(), "https://x/?a=b".to_string())
        );
    }

    #[test]
    fn metadata_accepts_an_empty_value_but_not_an_empty_key() {
        assert_eq!(
            parse_metadata_pair("cleared=").expect("parse"),
            ("cleared".to_string(), String::new())
        );
        assert!(parse_metadata_pair("=v").is_err());
        assert!(parse_metadata_pair("novalue").is_err());
    }

    #[test]
    fn no_metadata_pairs_leave_the_field_absent() {
        assert_eq!(collect_metadata(Vec::new()), None);
    }

    #[test]
    fn a_repeated_metadata_key_keeps_the_last_value() {
        let map = collect_metadata(vec![
            ("team".into(), "core".into()),
            ("team".into(), "platform".into()),
        ])
        .expect("metadata");
        assert_eq!(map["team"], "platform");
    }
}
