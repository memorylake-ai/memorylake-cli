//! Shared helpers for building API URL paths.

use percent_encoding::{AsciiSet, CONTROLS, utf8_percent_encode};

/// Control chars plus the URL-structural bytes we must escape when placing an
/// id into a path segment. Deliberately narrow so `ws-...` / `act-...` ids stay
/// readable in logs while `/`, `?`, spaces, etc. cannot corrupt the URL.
const PATH_ENCODE_SET: &AsciiSet = &CONTROLS
    .add(b' ')
    .add(b'"')
    .add(b'#')
    .add(b'%')
    .add(b'/')
    .add(b'<')
    .add(b'>')
    .add(b'?')
    .add(b'[')
    .add(b'\\')
    .add(b']')
    .add(b'^')
    .add(b'`')
    .add(b'{')
    .add(b'|')
    .add(b'}');

/// Percent-encode a caller-supplied id for safe use as one URL path segment.
pub(crate) fn encode_segment(segment: &str) -> String {
    utf8_percent_encode(segment, PATH_ENCODE_SET).to_string()
}
