//! Server-sent events (`text/event-stream`) parsing.
//!
//! The A2A streaming endpoints answer with one JSON document per event. Only
//! the `data:` field is meaningful there; `event:`, `id:` and `retry:` are
//! accepted and ignored so a server that starts sending them cannot break the
//! stream. The parser is a plain iterator over a `BufRead`, so it is driven by
//! the blocking HTTP response directly and tested with an in-memory cursor.

use std::io::BufRead;

use serde_json::Value;

use crate::error::{Error, Result};

/// Iterator over the JSON payloads of a `text/event-stream` body.
///
/// Each item is one event's `data` decoded as JSON. Events with no `data`
/// lines (keep-alive comments, bare `event:` lines) are skipped rather than
/// reported, since they carry nothing the caller can act on.
#[derive(Debug)]
pub struct EventStream<R> {
    reader: R,
    /// Set once the body ended or a read failed, so a caller that keeps
    /// polling after `None` gets `None` again rather than a fresh read.
    done: bool,
}

impl<R: BufRead> EventStream<R> {
    /// Wrap a body that is already positioned at the first event.
    pub fn new(reader: R) -> Self {
        Self {
            reader,
            done: false,
        }
    }

    /// Read lines up to the next blank line and collect the `data` field.
    ///
    /// Returns `Ok(None)` at end of body. A body that ends without a trailing
    /// blank line still yields its final event: A2A servers close the
    /// connection right after the last frame, and the event is complete
    /// either way.
    fn next_data(&mut self) -> Result<Option<String>> {
        let mut data: Vec<String> = Vec::new();
        let mut line = String::new();
        loop {
            line.clear();
            let read = self
                .reader
                .read_line(&mut line)
                .map_err(|source| Error::Io {
                    action: "read event stream",
                    path: std::path::PathBuf::from("<response>"),
                    source,
                })?;
            if read == 0 {
                self.done = true;
                return Ok((!data.is_empty()).then(|| data.join("\n")));
            }

            let line = line.trim_end_matches(['\r', '\n']);
            if line.is_empty() {
                if data.is_empty() {
                    // Blank line between events with nothing pending: a
                    // keep-alive or a stray separator. Keep reading.
                    continue;
                }
                return Ok(Some(data.join("\n")));
            }
            if line.starts_with(':') {
                // Comment line, used as a keep-alive by some servers.
                continue;
            }
            let (field, value) = match line.split_once(':') {
                Some((field, value)) => (field, value.strip_prefix(' ').unwrap_or(value)),
                None => (line, ""),
            };
            if field == "data" {
                data.push(value.to_string());
            }
        }
    }
}

impl<R: BufRead> Iterator for EventStream<R> {
    type Item = Result<Value>;

    fn next(&mut self) -> Option<Self::Item> {
        if self.done {
            return None;
        }
        let data = match self.next_data() {
            Err(err) => {
                self.done = true;
                return Some(Err(err));
            }
            Ok(None) => return None,
            Ok(Some(data)) => data,
        };
        let parsed = serde_json::from_str(&data).map_err(|source| Error::Api {
            message: format!("event stream sent a frame that is not JSON: {source}\n{data}"),
            code: None,
        });
        if parsed.is_err() {
            // A garbled frame means the rest cannot be trusted to line up
            // either; stop rather than resynchronise.
            self.done = true;
        }
        Some(parsed)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Cursor;

    fn events(body: &str) -> Vec<Value> {
        EventStream::new(Cursor::new(body.as_bytes()))
            .collect::<Result<Vec<_>>>()
            .expect("parse events")
    }

    #[test]
    fn splits_frames_on_blank_lines() {
        // Exactly the shape production sends: `data:` with no space, one
        // frame per blank-line-separated block.
        let body = "data:{\"a\":1}\n\ndata:{\"b\":2}\n\n";
        assert_eq!(
            events(body),
            vec![serde_json::json!({"a": 1}), serde_json::json!({"b": 2})]
        );
    }

    #[test]
    fn accepts_a_space_after_the_colon_and_crlf_line_endings() {
        let body = "data: {\"a\":1}\r\n\r\ndata: {\"b\":2}\r\n\r\n";
        assert_eq!(events(body).len(), 2);
    }

    #[test]
    fn joins_multi_line_data_with_newlines() {
        let body = "data:{\"text\":\ndata:\"x\"}\n\n";
        assert_eq!(events(body), vec![serde_json::json!({"text": "x"})]);
    }

    #[test]
    fn ignores_comments_ids_and_event_names() {
        let body = ": keep-alive\nevent: update\nid: 7\nretry: 100\ndata:{\"a\":1}\n\n: ping\n\n";
        assert_eq!(events(body), vec![serde_json::json!({"a": 1})]);
    }

    #[test]
    fn a_final_frame_without_a_trailing_blank_line_is_still_delivered() {
        let body = "data:{\"a\":1}\n\ndata:{\"b\":2}";
        assert_eq!(events(body).len(), 2);
    }

    #[test]
    fn an_empty_body_yields_nothing() {
        assert!(events("").is_empty());
        assert!(events("\n\n: only comments\n\n").is_empty());
    }

    #[test]
    fn a_non_json_frame_is_an_error_that_ends_the_stream() {
        let mut stream = EventStream::new(Cursor::new(b"data:not json\n\ndata:{\"a\":1}\n\n"));
        let err = stream.next().expect("one item").expect_err("not json");
        assert!(err.to_string().contains("not JSON"), "{err}");
        assert!(stream.next().is_none(), "stream must stop after an error");
    }
}
