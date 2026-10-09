//! Reading command input from a file or standard input.

use std::io::Read;
use std::path::Path;

use anyhow::{Context, Result};

/// Read the whole of `path`, or standard input when `path` is `-`.
///
/// `what` names the input in error messages ("instruction file", ...).
pub fn read_file_or_stdin(path: &Path, what: &str) -> Result<String> {
    if path.as_os_str() == "-" {
        let mut text = String::new();
        std::io::stdin()
            .read_to_string(&mut text)
            .with_context(|| format!("read {what} from standard input"))?;
        Ok(text)
    } else {
        std::fs::read_to_string(path).with_context(|| format!("read {what} {}", path.display()))
    }
}

/// Drop one trailing line ending (`\n` or `\r\n`), and nothing else.
///
/// `echo secret | ...` and most editors end a file with a newline that is not
/// part of the value. Any other whitespace is kept, since it may be.
pub fn strip_one_line_ending(text: &str) -> &str {
    text.strip_suffix("\r\n")
        .or_else(|| text.strip_suffix('\n'))
        .unwrap_or(text)
}

/// Clap value parser for a value that must not be empty or blank.
///
/// For names and ids: an empty one is never what was meant, and for some
/// fields (an analysis model id on update) the API reads `""` as an
/// instruction of its own.
pub fn parse_non_empty(raw: &str) -> std::result::Result<String, String> {
    if raw.trim().is_empty() {
        return Err("must not be empty".into());
    }
    Ok(raw.to_string())
}

/// Longest `custom_id` the database endpoints accept.
const MAX_CUSTOM_ID_CHARS: usize = 255;

/// Clap value parser for a database-resource `custom_id`.
///
/// The connection and datasource endpoints document a 255-character cap and
/// reject the reserved `_sys_` prefix; both are checked here so the mistake
/// costs no round trip.
pub fn parse_custom_id(raw: &str) -> std::result::Result<String, String> {
    parse_non_empty(raw)?;
    if raw.chars().count() > MAX_CUSTOM_ID_CHARS {
        return Err(format!("must be at most {MAX_CUSTOM_ID_CHARS} characters"));
    }
    if raw.starts_with("_sys_") {
        return Err("the `_sys_` prefix is reserved".into());
    }
    Ok(raw.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn custom_ids_are_checked_against_the_documented_rules() {
        assert_eq!(parse_custom_id("conn-1").as_deref(), Ok("conn-1"));
        assert!(parse_custom_id(&"x".repeat(255)).is_ok());
        assert!(parse_custom_id(&"x".repeat(256)).is_err());
        assert!(parse_custom_id("_sys_mine").is_err());
    }

    #[test]
    fn blank_values_are_refused() {
        assert_eq!(parse_non_empty("db-1").as_deref(), Ok("db-1"));
        assert!(parse_non_empty("").is_err());
        assert!(parse_non_empty("  \t").is_err());
        assert!(parse_custom_id("").is_err());
    }

    #[test]
    fn exactly_one_line_ending_is_dropped() {
        assert_eq!(strip_one_line_ending("secret\n"), "secret");
        assert_eq!(strip_one_line_ending("secret\r\n"), "secret");
        assert_eq!(strip_one_line_ending("secret\n\n"), "secret\n");
        assert_eq!(strip_one_line_ending("secret"), "secret");
    }

    #[test]
    fn other_whitespace_is_part_of_the_value() {
        assert_eq!(strip_one_line_ending("  pass word \n"), "  pass word ");
    }
}
