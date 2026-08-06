//! Chunked file upload (`POST /api/v1/drives/items/upload` + finalize).
//!
//! The protocol has three stages:
//!
//! 1. Create a session for the file's exact byte length. The server answers
//!    with a part plan: how many parts, how large each is, and a pre-signed URL
//!    for each.
//! 2. `PUT` each part's bytes to its URL, unauthenticated, and keep the `ETag`
//!    the storage backend returns.
//! 3. Finalize via [`create_file`](super::create_file), which is what actually
//!    makes the file appear in the Library.
//!
//! Part sizes are chosen by the server and vary with file size (5 MiB parts
//! were observed up to ~100 MiB, 10 MiB at 1 GiB). Never assume a fixed chunk
//! size — always follow the returned plan.

use std::fs::File;
use std::io::{Read, Seek, SeekFrom};
use std::path::{Path, PathBuf};
use std::thread::sleep;
use std::time::Duration;

use reqwest::blocking::Body;
use serde::{Deserialize, Serialize};

use crate::client::Client;
use crate::error::{Error, Result};

use super::create::{CreateFileRequest, PartETag, create_file};
use super::paths::upload_path;
use super::types::{CreatedItem, NameConflictStrategy};

/// Attempts made per part before giving up, counting the first try.
const MAX_PART_ATTEMPTS: u32 = 4;

/// Delay before the second attempt; doubles for each attempt after that.
const INITIAL_RETRY_DELAY: Duration = Duration::from_millis(500);

/// One part of a server-issued upload plan.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PartItem {
    /// 1-based part number. Must be echoed back when finalizing.
    pub number: u32,
    /// Exact byte count this part must carry.
    pub size: u64,
    /// Pre-signed URL to `PUT` the bytes to. Carries its own credentials and
    /// expires; do not add an `Authorization` header.
    pub upload_url: String,
}

/// A chunked upload session and its part plan.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct UploadSession {
    /// Opaque session handle, passed back when finalizing.
    pub upload_id: String,
    /// Parts to upload, in server-assigned order.
    #[serde(default)]
    pub part_items: Vec<PartItem>,
}

#[derive(Debug, Serialize)]
struct CreateUploadBody {
    file_size: u64,
}

/// Start a chunked upload session for a file of `file_size` bytes.
///
/// Uploads no data; the returned plan describes what to send. `file_size` must
/// be at least 1.
pub fn create_upload_session(client: &Client, file_size: u64) -> Result<UploadSession> {
    client.post_data(&upload_path(), &CreateUploadBody { file_size })
}

/// Upload a local file into the Library.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct UploadFileRequest {
    /// Local file to read.
    pub source: PathBuf,
    /// Destination folder id, or [`ROOT_ALIAS`](super::ROOT_ALIAS).
    pub parent_item_id: String,
    /// Name to give the item in the Library.
    pub name: String,
    /// Behavior on name collision. `None` uses the server default (`rename`).
    pub name_conflict_strategy: Option<NameConflictStrategy>,
}

/// Upload `request.source` and register it as a Library file.
///
/// Streams the file one part at a time; memory use is bounded by the HTTP
/// stack, not by the file size. Individual parts are retried a bounded number
/// of times on transport failures, HTTP 5xx, and HTTP 429. Any other rejection
/// — including the 403 an expired pre-signed URL produces — fails immediately,
/// because the signature cannot change within a session.
///
/// The returned [`CreatedItem::name`] is authoritative: under the default
/// `rename` strategy the server may have appended a `_N` suffix.
pub fn upload_file(client: &Client, request: &UploadFileRequest) -> Result<CreatedItem> {
    let source = request.source.as_path();
    let file_size = file_len(source)?;
    if file_size == 0 {
        return Err(Error::EmptyUpload {
            path: source.to_path_buf(),
        });
    }

    let session = create_upload_session(client, file_size)?;
    validate_plan(&session, source, file_size)?;

    let total = session.part_items.len() as u32;
    let offsets = part_offsets(&session.part_items);
    let mut part_etags = Vec::with_capacity(session.part_items.len());

    for (part, offset) in session.part_items.iter().zip(offsets) {
        // A file that shrinks mid-upload would otherwise finalize into an item
        // whose ETags describe bytes that no longer exist.
        let current = file_len(source)?;
        if current != file_size {
            return Err(Error::UploadSizeChanged {
                path: source.to_path_buf(),
                expected: file_size,
                actual: current,
            });
        }

        tracing::debug!(
            part = part.number,
            total,
            offset,
            size = part.size,
            "uploading part"
        );
        let etag = upload_part(client, source, part, offset, total)?;
        part_etags.push(PartETag {
            number: part.number,
            etag,
        });
    }

    create_file(
        client,
        &CreateFileRequest {
            parent_item_id: request.parent_item_id.clone(),
            name: request.name.clone(),
            upload_id: session.upload_id,
            part_etags,
            name_conflict_strategy: request.name_conflict_strategy,
        },
    )
}

/// Send one part, retrying only failures that a further attempt could fix.
fn upload_part(
    client: &Client,
    source: &Path,
    part: &PartItem,
    offset: u64,
    total: u32,
) -> Result<String> {
    let mut delay = INITIAL_RETRY_DELAY;
    let mut attempt = 1;

    // Every arm returns or loops, so there is no fall-through path to get wrong
    // if the attempt budget is ever changed.
    loop {
        // Re-open per attempt so a partially consumed reader can never be
        // resent from the wrong position.
        let body = part_body(source, offset, part.size)?;

        match client.put_presigned_part(&part.upload_url, body) {
            Ok(etag) => return Ok(etag),
            Err(err) if err.is_retryable() && attempt < MAX_PART_ATTEMPTS => {
                tracing::warn!(
                    part = part.number,
                    attempt,
                    max = MAX_PART_ATTEMPTS,
                    retry_in_ms = delay.as_millis(),
                    error = %err,
                    "part upload failed; retrying"
                );
                sleep(delay);
                delay *= 2;
                attempt += 1;
            }
            Err(err) => {
                // A refused (non-retryable) status means the signature itself
                // was rejected, so the whole session has to be redone.
                return match err.status().filter(|_| !err.is_retryable()) {
                    Some(status) => Err(Error::UploadUrlRefused {
                        path: source.to_path_buf(),
                        number: part.number,
                        status: status.as_u16(),
                    }),
                    None => Err(Error::PartUpload {
                        path: source.to_path_buf(),
                        number: part.number,
                        total,
                        attempts: attempt,
                        source: err,
                    }),
                };
            }
        }
    }
}

/// Starting byte offset of each part, accumulated in plan order.
///
/// The plan gives sizes but no offsets, so a part's position is the sum of
/// everything before it. Getting this wrong sends valid-looking bytes to the
/// wrong part and produces a corrupt file that still finalizes cleanly.
fn part_offsets(parts: &[PartItem]) -> Vec<u64> {
    parts
        .iter()
        .scan(0u64, |offset, part| {
            let start = *offset;
            *offset += part.size;
            Some(start)
        })
        .collect()
}

/// Build a streaming body for exactly `size` bytes starting at `offset`.
fn part_body(source: &Path, offset: u64, size: u64) -> Result<Body> {
    let mut file = File::open(source).map_err(|source_err| Error::Io {
        action: "open",
        path: source.to_path_buf(),
        source: source_err,
    })?;
    file.seek(SeekFrom::Start(offset))
        .map_err(|source_err| Error::Io {
            action: "seek",
            path: source.to_path_buf(),
            source: source_err,
        })?;
    // `take` bounds the read so the body can never overrun into the next part.
    Ok(Body::sized(file.take(size), size))
}

fn file_len(path: &Path) -> Result<u64> {
    let metadata = std::fs::metadata(path).map_err(|source| Error::Io {
        action: "stat",
        path: path.to_path_buf(),
        source,
    })?;
    Ok(metadata.len())
}

/// Reject a plan that does not add up to the file we are about to send.
///
/// A mismatch here would produce a corrupt item rather than a clean failure, so
/// it is checked before any bytes leave the machine.
fn validate_plan(session: &UploadSession, source: &Path, file_size: u64) -> Result<()> {
    let reason = if session.upload_id.trim().is_empty() {
        Some("session has no upload_id".to_string())
    } else if session.part_items.is_empty() {
        Some("plan contains no parts".to_string())
    } else if let Some(bad) = session
        .part_items
        .iter()
        .enumerate()
        .find(|(index, part)| part.number != *index as u32 + 1)
    {
        Some(format!(
            "part numbers are not a contiguous 1-based sequence (position {} is numbered {})",
            bad.0 + 1,
            bad.1.number
        ))
    } else if session
        .part_items
        .iter()
        .any(|part| part.upload_url.is_empty())
    {
        Some("a part is missing its upload URL".to_string())
    } else {
        let planned: u64 = session.part_items.iter().map(|part| part.size).sum();
        (planned != file_size)
            .then(|| format!("parts total {planned} bytes but the file is {file_size} bytes"))
    };

    match reason {
        Some(reason) => Err(Error::UploadPlan {
            path: source.to_path_buf(),
            reason,
        }),
        None => Ok(()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    use crate::test_support::one_shot_server;

    fn temp_dir(tag: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!(
            "mlcli-upload-{tag}-{}-{:?}",
            std::process::id(),
            std::thread::current().id()
        ));
        std::fs::create_dir_all(&dir).expect("create temp dir");
        dir
    }

    fn session(parts: &[(u32, u64)]) -> UploadSession {
        UploadSession {
            upload_id: "u-1".into(),
            part_items: parts
                .iter()
                .map(|(number, size)| PartItem {
                    number: *number,
                    size: *size,
                    upload_url: format!("https://storage.invalid/p{number}"),
                })
                .collect(),
        }
    }

    fn plan_error(session: &UploadSession, file_size: u64) -> String {
        match validate_plan(session, Path::new("/tmp/x.bin"), file_size) {
            Err(Error::UploadPlan { reason, .. }) => reason,
            other => panic!("expected an UploadPlan error, got {other:?}"),
        }
    }

    #[test]
    fn accepts_a_plan_that_matches_the_file() {
        // Mirrors a real 6 MiB upload: a full 5 MiB part plus a 1 MiB remainder.
        let session = session(&[(1, 5_242_880), (2, 1_048_576)]);
        assert!(validate_plan(&session, Path::new("/tmp/x.bin"), 6_291_456).is_ok());
    }

    #[test]
    fn rejects_a_plan_whose_sizes_do_not_sum_to_the_file() {
        let session = session(&[(1, 5_242_880), (2, 1_048_576)]);
        assert!(plan_error(&session, 7_000_000).contains("parts total 6291456 bytes"));
    }

    #[test]
    fn rejects_non_contiguous_part_numbers() {
        // Numbers are echoed back verbatim at finalize; a gap would pair ETags
        // with the wrong bytes.
        let session = session(&[(1, 10), (3, 10)]);
        assert!(plan_error(&session, 20).contains("contiguous"));
    }

    #[test]
    fn rejects_an_empty_plan() {
        let session = session(&[]);
        assert!(plan_error(&session, 10).contains("no parts"));
    }

    #[test]
    fn rejects_a_session_without_an_upload_id() {
        let mut session = session(&[(1, 10)]);
        session.upload_id = "  ".into();
        assert!(plan_error(&session, 10).contains("upload_id"));
    }

    #[test]
    fn offsets_accumulate_in_plan_order() {
        let parts = session(&[(1, 5_242_880), (2, 5_242_880), (3, 1_048_576)]).part_items;
        assert_eq!(part_offsets(&parts), vec![0, 5_242_880, 10_485_760]);
    }

    #[test]
    fn offsets_are_empty_for_an_empty_plan() {
        assert!(part_offsets(&[]).is_empty());
    }

    #[test]
    fn upload_part_sends_exactly_its_slice_of_the_file() {
        let dir = temp_dir("slice");
        let path = dir.join("x.bin");
        std::fs::write(&path, b"0123456789").unwrap();

        let (base, server) = one_shot_server("HTTP/1.1 200 OK\r\nETag: \"e2\"\r\n\r\n");
        let client = Client::new("http://unused.invalid", "sk_test_key_abcdefghij").unwrap();
        let part = PartItem {
            number: 2,
            size: 4,
            upload_url: format!("{base}/p2"),
        };

        let etag = upload_part(&client, &path, &part, 3, 3).expect("upload part");
        assert_eq!(etag, "\"e2\"");

        // The bytes on the wire must be the part's own window, not the file
        // head and not a byte more than `size`.
        let request = server.join().expect("server thread");
        assert_eq!(request.body, b"3456");
        assert!(!request.has_header("authorization"));

        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn upload_part_reports_a_refused_url_as_needing_a_new_session() {
        let dir = temp_dir("refused");
        let path = dir.join("x.bin");
        std::fs::write(&path, b"0123456789").unwrap();

        let (base, server) =
            one_shot_server("HTTP/1.1 403 Forbidden\r\nContent-Length: 16\r\n\r\nRequest expired.");
        let client = Client::new("http://unused.invalid", "sk_test_key_abcdefghij").unwrap();
        let part = PartItem {
            number: 1,
            size: 4,
            upload_url: format!("{base}/p1"),
        };

        // Only one connection is served: proving no retry happened is the point.
        let err = upload_part(&client, &path, &part, 0, 1).expect_err("403 fails the part");
        match err {
            Error::UploadUrlRefused { number, status, .. } => {
                assert_eq!(number, 1);
                assert_eq!(status, 403);
                assert!(err.to_string().contains("re-run the upload"));
            }
            other => panic!("expected UploadUrlRefused, got {other:?}"),
        }

        let _ = server.join();
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn upload_file_rejects_an_empty_file_before_any_request() {
        let dir = temp_dir("empty");
        let path = dir.join("empty.bin");
        std::fs::write(&path, b"").unwrap();

        // Base URL is unroutable: reaching the network at all would fail
        // differently, so an EmptyUpload proves the short-circuit.
        let client = Client::new("http://unused.invalid", "sk_test_key_abcdefghij").unwrap();
        let err = upload_file(
            &client,
            &UploadFileRequest {
                source: path.clone(),
                parent_item_id: "MY_SPACE".into(),
                name: "empty.bin".into(),
                name_conflict_strategy: None,
            },
        )
        .expect_err("empty file is rejected");

        assert!(matches!(err, Error::EmptyUpload { .. }));
        let _ = std::fs::remove_dir_all(&dir);
    }
}
