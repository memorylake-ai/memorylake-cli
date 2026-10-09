//! Tolerant decoders for informational response fields.
//!
//! One odd value in one item must not fail a whole page. These are for fields
//! the CLI only reports — counts, ports, versions — never for anything a
//! decision is made on.

use serde::{Deserialize, Deserializer};
use serde_json::Value;

/// Decode an integer field, accepting a number or a numeric string.
///
/// Anything else — a fraction, a value outside `i64`, a boolean, an object —
/// decodes as `None` rather than failing the response.
pub(crate) fn int<'de, D>(deserializer: D) -> Result<Option<i64>, D::Error>
where
    D: Deserializer<'de>,
{
    Ok(match Option::<Value>::deserialize(deserializer)? {
        Some(Value::Number(number)) => number.as_i64(),
        Some(Value::String(text)) => text.trim().parse().ok(),
        _ => None,
    })
}

/// Decode a version field, accepting a bare number as well as a string.
///
/// The published examples write build versions as 17-digit numbers although
/// the field is typed string. A number keeps its exact digits; any other
/// non-string value decodes as `None`.
pub(crate) fn version<'de, D>(deserializer: D) -> Result<Option<String>, D::Error>
where
    D: Deserializer<'de>,
{
    Ok(match Option::<Value>::deserialize(deserializer)? {
        Some(Value::String(version)) => Some(version),
        Some(Value::Number(version)) => Some(version.to_string()),
        _ => None,
    })
}

#[cfg(test)]
mod tests {
    use serde::Deserialize;

    #[derive(Debug, Deserialize)]
    struct Probe {
        #[serde(default, deserialize_with = "super::int")]
        n: Option<i64>,
        #[serde(default, deserialize_with = "super::version")]
        v: Option<String>,
    }

    fn probe(json: &str) -> Probe {
        serde_json::from_str(json).unwrap_or_else(|err| panic!("{json}: {err}"))
    }

    #[test]
    fn integers_decode_from_numbers_and_numeric_strings() {
        assert_eq!(probe(r#"{"n":5432}"#).n, Some(5432));
        assert_eq!(probe(r#"{"n":-1}"#).n, Some(-1));
        assert_eq!(probe(r#"{"n":"70000"}"#).n, Some(70_000));
        assert_eq!(probe("{}").n, None);
        assert_eq!(probe(r#"{"n":null}"#).n, None);
    }

    #[test]
    fn odd_integers_become_absent_instead_of_failing() {
        for json in [
            r#"{"n":1.5}"#,
            r#"{"n":"many"}"#,
            r#"{"n":true}"#,
            r#"{"n":{"x":1}}"#,
            r#"{"n":18446744073709551615}"#,
        ] {
            assert_eq!(probe(json).n, None, "{json}");
        }
    }

    #[test]
    fn versions_keep_their_exact_digits() {
        assert_eq!(
            probe(r#"{"v":20260401023011680}"#).v.as_deref(),
            Some("20260401023011680")
        );
        assert_eq!(probe(r#"{"v":"abc"}"#).v.as_deref(), Some("abc"));
        assert_eq!(probe(r#"{"v":[1]}"#).v, None);
    }
}
