//! Shared helpers for building API URL paths.

use percent_encoding::{AsciiSet, CONTROLS, utf8_percent_encode};

/// Control chars plus the URL-structural bytes we must escape when placing an
/// id into a path segment. Deliberately narrow so `ws-...` / `act-...` /
/// `sc-...` ids stay readable in logs while `/`, `?`, spaces, etc. cannot
/// corrupt the URL.
///
/// `:` is deliberately absent: it is a legal `pchar` (RFC 3986 §3.3) and every
/// Library item id embeds one (`sc-<hash>:inode-<hash>`). Encoding it would
/// change the resource being addressed.
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn leaves_typical_ids_untouched() {
        assert_eq!(
            encode_segment("ws-b83fa7f09f19487f9905888f35542849"),
            "ws-b83fa7f09f19487f9905888f35542849"
        );
        assert_eq!(
            encode_segment("_sys_default_workspace"),
            "_sys_default_workspace"
        );
    }

    #[test]
    fn preserves_colon_in_library_item_ids() {
        // Library ids are `<space>:<inode>`; a percent-encoded `:` addresses a
        // different resource and the API rejects it.
        assert_eq!(
            encode_segment("sc-d68ddc76b98c4df4a3002cd53aecfc5b:inode-043abbb41bbd449fae79404c6"),
            "sc-d68ddc76b98c4df4a3002cd53aecfc5b:inode-043abbb41bbd449fae79404c6"
        );
    }

    #[test]
    fn escapes_url_structural_chars() {
        assert_eq!(
            encode_segment("weird id/here?foo#bar"),
            "weird%20id%2Fhere%3Ffoo%23bar"
        );
    }

    #[test]
    fn escapes_percent_itself() {
        // A stray `%` must be encoded so it can't be misread as a pct-triplet.
        assert_eq!(encode_segment("100%off"), "100%25off");
    }
}
