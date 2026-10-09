//! A minimal ZIP writer for skill package fixtures.
//!
//! The CLI deliberately takes a ready-made archive rather than zipping a
//! directory itself, and the test suite has no zip dependency, so fixtures are
//! built by hand: stored (uncompressed) entries, a central directory, and its
//! end record.

/// CRC-32 (IEEE), as ZIP requires. Bitwise: the inputs here are tiny.
fn crc32(bytes: &[u8]) -> u32 {
    let mut crc = 0xFFFF_FFFFu32;
    for &byte in bytes {
        crc ^= u32::from(byte);
        for _ in 0..8 {
            crc = if crc & 1 == 1 {
                (crc >> 1) ^ 0xEDB8_8320
            } else {
                crc >> 1
            };
        }
    }
    !crc
}

/// A ZIP archive storing `files` uncompressed.
pub fn stored_zip(files: &[(&str, &[u8])]) -> Vec<u8> {
    const DOS_DATE_1980_01_01: u16 = 0x21;
    let mut out = Vec::new();
    let mut central = Vec::new();

    for (name, data) in files {
        let offset = out.len() as u32;
        let crc = crc32(data);
        let size = data.len() as u32;
        let name_len = name.len() as u16;

        out.extend_from_slice(&0x0403_4b50u32.to_le_bytes());
        for field in [20u16, 0, 0, 0, DOS_DATE_1980_01_01] {
            out.extend_from_slice(&field.to_le_bytes());
        }
        for field in [crc, size, size] {
            out.extend_from_slice(&field.to_le_bytes());
        }
        out.extend_from_slice(&name_len.to_le_bytes());
        out.extend_from_slice(&0u16.to_le_bytes());
        out.extend_from_slice(name.as_bytes());
        out.extend_from_slice(data);

        central.extend_from_slice(&0x0201_4b50u32.to_le_bytes());
        for field in [20u16, 20, 0, 0, 0, DOS_DATE_1980_01_01] {
            central.extend_from_slice(&field.to_le_bytes());
        }
        for field in [crc, size, size] {
            central.extend_from_slice(&field.to_le_bytes());
        }
        for field in [name_len, 0, 0, 0, 0] {
            central.extend_from_slice(&field.to_le_bytes());
        }
        central.extend_from_slice(&0u32.to_le_bytes());
        central.extend_from_slice(&offset.to_le_bytes());
        central.extend_from_slice(name.as_bytes());
    }

    let central_offset = out.len() as u32;
    let entries = files.len() as u16;
    out.extend_from_slice(&central);
    out.extend_from_slice(&0x0605_4b50u32.to_le_bytes());
    for field in [0u16, 0, entries, entries] {
        out.extend_from_slice(&field.to_le_bytes());
    }
    out.extend_from_slice(&(central.len() as u32).to_le_bytes());
    out.extend_from_slice(&central_offset.to_le_bytes());
    out.extend_from_slice(&0u16.to_le_bytes());
    out
}

#[test]
fn crc32_matches_the_reference_value() {
    // The standard check value for CRC-32/IEEE.
    assert_eq!(crc32(b"123456789"), 0xCBF4_3926);
}

#[test]
fn stored_zip_has_the_local_header_and_end_record() {
    let archive = stored_zip(&[("SKILL.md", b"hi")]);
    assert!(archive.starts_with(b"PK\x03\x04"));
    // The end-of-central-directory record is the last 22 bytes.
    assert_eq!(
        &archive[archive.len() - 22..archive.len() - 18],
        b"PK\x05\x06"
    );
}
