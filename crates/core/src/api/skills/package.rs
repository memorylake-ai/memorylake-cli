//! Upload a skill package (`POST /api/v3/skills/package-uploads`, then a PUT
//! to the pre-signed URL it returns).

use std::collections::BTreeMap;
use std::fmt;
use std::io::Read;
use std::path::Path;
use std::time::Duration;

use reqwest::blocking::Body;
use serde::Deserialize;
use serde_json::{Map, Value};

use crate::client::Client;
use crate::error::{Error, Result};

use super::PACKAGE_UPLOADS_PATH;

/// Largest package the server accepts: 10 MiB.
///
/// The server checks this only when the package is *used* (create or publish),
/// after the upload has already gone through, so checking locally first saves
/// a pointless transfer.
pub const MAX_PACKAGE_BYTES: u64 = 10 * 1024 * 1024;

/// Content type a skill package is uploaded as. ZIP is the only format.
const PACKAGE_CONTENT_TYPE: &str = "application/zip";

/// Leading bytes of a ZIP archive: a local file header, an empty archive's
/// end-of-central-directory record, or a spanned-archive marker.
const ZIP_SIGNATURES: [[u8; 4]; 3] = [*b"PK\x03\x04", *b"PK\x05\x06", *b"PK\x07\x08"];

/// An upload slot for one skill package.
#[derive(Clone, PartialEq, Eq, Deserialize)]
pub struct PackageUploadSlot {
    /// Pre-signed URL to PUT the archive to. A working credential until it
    /// expires, so it is never printed by [`fmt::Debug`].
    pub upload_url: String,
    /// Headers the PUT must carry, verbatim. Always includes
    /// `Content-Type: application/zip`.
    #[serde(default)]
    pub upload_headers: BTreeMap<String, String>,
    /// Storage URI to pass back as `package_ref.s3_uri`. Opaque.
    pub s3_uri: String,
    /// Seconds until `upload_url` expires (1800 measured 2026-10-09).
    #[serde(default)]
    pub expires_in_seconds: Option<u64>,
    /// Fields returned by the server that this client does not model.
    #[serde(flatten)]
    pub extra: Map<String, Value>,
}

impl fmt::Debug for PackageUploadSlot {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("PackageUploadSlot")
            .field("upload_url", &"<redacted>")
            .field("upload_headers", &self.upload_headers)
            .field("s3_uri", &self.s3_uri)
            .field("expires_in_seconds", &self.expires_in_seconds)
            .finish_non_exhaustive()
    }
}

/// Request an upload slot for a skill package.
///
/// The endpoint takes no body; `{}` is sent because the client always posts
/// JSON, and production accepts it (measured 2026-10-09).
pub fn create_package_upload(client: &Client) -> Result<PackageUploadSlot> {
    client.post_data(PACKAGE_UPLOADS_PATH, &Map::new())
}

/// Check that `path` looks like a usable skill package and return its size.
///
/// Catches what can be caught without the network: a directory, an empty or
/// oversized file, or a file that is not a ZIP archive. Whether the archive
/// holds a `SKILL.md` is left to the server, which reports it precisely.
pub fn validate_package(path: &Path) -> Result<u64> {
    let invalid = |reason: String| Error::InvalidSkillPackage {
        path: path.to_path_buf(),
        reason,
    };

    let metadata = std::fs::metadata(path).map_err(|source| Error::Io {
        action: "read",
        path: path.to_path_buf(),
        source,
    })?;
    if metadata.is_dir() {
        return Err(invalid(
            "it is a directory; zip it first (e.g. `cd dir && zip -r ../skill.zip .`) and pass the .zip"
                .into(),
        ));
    }
    let size = metadata.len();
    if size == 0 {
        return Err(invalid("the file is empty".into()));
    }
    if size > MAX_PACKAGE_BYTES {
        return Err(invalid(format!(
            "it is {size} bytes; the server accepts at most {MAX_PACKAGE_BYTES} (10 MiB)"
        )));
    }

    let mut magic = [0u8; 4];
    let read = std::fs::File::open(path)
        .and_then(|mut file| read_prefix(&mut file, &mut magic))
        .map_err(|source| Error::Io {
            action: "read",
            path: path.to_path_buf(),
            source,
        })?;
    if read < magic.len() || !ZIP_SIGNATURES.contains(&magic) {
        return Err(invalid(
            "it is not a ZIP archive; ZIP is the only accepted package format".into(),
        ));
    }
    Ok(size)
}

/// Upload the ZIP archive at `path` and return the storage URI to reference it
/// by when creating a skill or publishing a version.
///
/// The archive is read into memory (it is at most [`MAX_PACKAGE_BYTES`]) so the
/// PUT carries a `Content-Length`; storage rejects a chunked upload.
pub fn upload_package(client: &Client, path: &Path) -> Result<String> {
    validate_package(path)?;
    let bytes = std::fs::read(path).map_err(|source| Error::Io {
        action: "read",
        path: path.to_path_buf(),
        source,
    })?;

    let timeout = upload_timeout(bytes.len() as u64);
    let slot = create_package_upload(client)?;
    client.put_presigned_object(
        &slot.upload_url,
        &slot.upload_headers,
        PACKAGE_CONTENT_TYPE,
        Body::from(bytes),
        timeout,
    )?;
    Ok(slot.s3_uri)
}

/// How long the package PUT may take.
///
/// The client's default timeout suits small JSON calls, not a 10 MiB body on a
/// slow link. Allow a fixed minute plus one second per 32 KiB, i.e. assume no
/// worse than ~32 KiB/s: a full-size package gets a little over six minutes.
fn upload_timeout(bytes: u64) -> Duration {
    const BASE: Duration = Duration::from_secs(60);
    const BYTES_PER_SECOND: u64 = 32 * 1024;
    BASE + Duration::from_secs(bytes.div_ceil(BYTES_PER_SECOND))
}

/// Fill `buf` from `reader`, stopping early only at end of input.
fn read_prefix(reader: &mut impl Read, buf: &mut [u8]) -> std::io::Result<usize> {
    let mut filled = 0;
    while filled < buf.len() {
        match reader.read(&mut buf[filled..])? {
            0 => break,
            read => filled += read,
        }
    }
    Ok(filled)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    use crate::test_support::{json_ok, one_shot_server};

    /// A scratch `skill.zip`, removed with its directory when dropped.
    struct TempPackage {
        dir: PathBuf,
        path: PathBuf,
    }

    impl Drop for TempPackage {
        fn drop(&mut self) {
            let _ = std::fs::remove_dir_all(&self.dir);
        }
    }

    fn temp_file(tag: &str, contents: &[u8]) -> TempPackage {
        let dir = std::env::temp_dir().join(format!(
            "memorylake-skill-package-{tag}-{}",
            std::process::id()
        ));
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("skill.zip");
        std::fs::write(&path, contents).unwrap();
        TempPackage { dir, path }
    }

    fn reason(err: Error) -> String {
        match err {
            Error::InvalidSkillPackage { reason, .. } => reason,
            other => panic!("unexpected error: {other:?}"),
        }
    }

    #[test]
    fn accepts_a_zip_and_reports_its_size() {
        let package = temp_file("ok", b"PK\x03\x04rest-of-archive");
        assert_eq!(validate_package(&package.path).unwrap(), 19);
    }

    #[test]
    fn rejects_a_file_that_is_not_a_zip() {
        let package = temp_file("notzip", b"# SKILL.md\n");
        assert!(reason(validate_package(&package.path).unwrap_err()).contains("not a ZIP"));
    }

    #[test]
    fn rejects_a_file_shorter_than_the_signature() {
        let package = temp_file("short", b"PK");
        assert!(reason(validate_package(&package.path).unwrap_err()).contains("not a ZIP"));
    }

    #[test]
    fn rejects_an_empty_file() {
        let package = temp_file("empty", b"");
        assert!(reason(validate_package(&package.path).unwrap_err()).contains("empty"));
    }

    #[test]
    fn rejects_a_directory_with_a_hint() {
        let dir = std::env::temp_dir();
        let message = reason(validate_package(&dir).unwrap_err());
        assert!(message.contains("directory"), "{message}");
        assert!(message.contains("zip -r"), "{message}");
    }

    #[test]
    fn rejects_an_oversized_file() {
        let package = temp_file("big", b"PK\x03\x04");
        let file = std::fs::OpenOptions::new()
            .write(true)
            .open(&package.path)
            .unwrap();
        file.set_len(MAX_PACKAGE_BYTES + 1).unwrap();
        assert!(reason(validate_package(&package.path).unwrap_err()).contains("at most"));
    }

    #[test]
    fn a_missing_file_is_an_io_error() {
        let path = std::env::temp_dir().join("memorylake-skill-package-does-not-exist.zip");
        assert!(matches!(
            validate_package(&path).unwrap_err(),
            Error::Io { .. }
        ));
    }

    #[test]
    fn upload_timeout_grows_with_the_package() {
        assert_eq!(upload_timeout(0), Duration::from_secs(60));
        assert_eq!(upload_timeout(1), Duration::from_secs(61));
        assert_eq!(
            upload_timeout(MAX_PACKAGE_BYTES),
            Duration::from_secs(60 + 320)
        );
    }

    #[test]
    fn slot_decodes_and_debug_hides_the_signed_url() {
        let slot: PackageUploadSlot = serde_json::from_str(
            r#"{"upload_url":"https://s3/x?X-Amz-Signature=secret",
                "upload_headers":{"Content-Type":"application/zip"},
                "s3_uri":"s3://b/tmp/x.zip","expires_in_seconds":1800}"#,
        )
        .unwrap();
        assert_eq!(slot.s3_uri, "s3://b/tmp/x.zip");
        assert_eq!(slot.upload_headers["Content-Type"], "application/zip");
        assert!(!format!("{slot:?}").contains("secret"));
    }

    #[test]
    fn slot_request_posts_to_the_package_uploads_path() {
        let (base, server) = one_shot_server(json_ok(
            r#"{"success":true,"data":{"upload_url":"http://u","upload_headers":{},"s3_uri":"s3://b/k.zip"}}"#,
        ));
        let client = Client::new(base, "sk_test_key_abcdefghij").unwrap();
        let slot = create_package_upload(&client).unwrap();
        assert_eq!(slot.s3_uri, "s3://b/k.zip");

        let request = server.join().unwrap();
        assert!(
            request
                .head
                .starts_with("POST /api/v3/skills/package-uploads "),
            "{}",
            request.head
        );
    }
}
