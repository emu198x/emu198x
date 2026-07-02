//! Commodore 64 `T64` host-side tape-container parsing.
//!
//! `T64` is not raw datasette pulse media. It is a file container that stores
//! one or more C64 program entries with load addresses and payload offsets.

use thiserror::Error;

const HEADER_LEN: usize = 0x40;
const ENTRY_LEN: usize = 0x20;

/// One loadable program entry extracted from a `T64` container.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct T64Program {
    /// Decoded entry name.
    pub name: String,
    /// Start address recorded in the archive entry.
    pub start_address: u16,
    /// End address recorded in the archive entry.
    pub end_address: u16,
    /// Program payload bytes without the PRG load-address prefix.
    pub data: Vec<u8>,
}

impl T64Program {
    /// Returns this entry as a C64 PRG byte stream.
    #[must_use]
    pub fn prg_bytes(&self) -> Vec<u8> {
        let mut bytes = Vec::with_capacity(self.data.len() + 2);
        bytes.extend_from_slice(&self.start_address.to_le_bytes());
        bytes.extend_from_slice(&self.data);
        bytes
    }
}

/// Error surfaced while parsing one `T64` container.
#[derive(Debug, Error, PartialEq, Eq)]
pub enum T64ParseError {
    /// The container is shorter than the fixed header.
    #[error("T64 image is too short: expected at least {expected} bytes, got {actual}")]
    TooShort { expected: usize, actual: usize },

    /// The 32-byte signature is not recognized.
    #[error("T64 image has an invalid header signature")]
    InvalidSignature,

    /// One entry points outside the image.
    #[error("T64 entry data extends beyond the image")]
    TruncatedEntryData,

    /// One entry advertises an invalid address range.
    #[error("T64 entry has an invalid address range ${start:04X}-${end:04X}")]
    InvalidAddressRange { start: u16, end: u16 },

    /// No loadable entries were present.
    #[error("T64 image does not contain any loadable program entries")]
    NoLoadableEntries,
}

/// Extracts the first loadable program entry from one `T64` image.
///
/// # Errors
///
/// Returns an error if the container is malformed or contains no loadable
/// entries.
pub fn extract_first_program(bytes: &[u8]) -> Result<T64Program, T64ParseError> {
    let used_entries = validate_header(bytes)?;
    for index in 0..used_entries {
        let base = HEADER_LEN + index * ENTRY_LEN;
        if base + ENTRY_LEN > bytes.len() {
            break;
        }
        if let Some(program) = parse_entry(bytes, base)? {
            return Ok(program);
        }
    }
    Err(T64ParseError::NoLoadableEntries)
}

/// Extracts every loadable program entry from one `T64` image, in directory
/// order. `T64` is a multi-file container; most carry a single program, but
/// compilations and demo collections hold several.
///
/// # Errors
///
/// Returns an error if the container header is malformed, an entry advertises
/// an invalid range, or an entry's payload extends past the image. Returns
/// [`T64ParseError::NoLoadableEntries`] if no type-1 entries are present.
pub fn extract_programs(bytes: &[u8]) -> Result<Vec<T64Program>, T64ParseError> {
    let used_entries = validate_header(bytes)?;
    let mut programs = Vec::new();
    for index in 0..used_entries {
        let base = HEADER_LEN + index * ENTRY_LEN;
        if base + ENTRY_LEN > bytes.len() {
            break;
        }
        if let Some(program) = parse_entry(bytes, base)? {
            programs.push(program);
        }
    }
    if programs.is_empty() {
        return Err(T64ParseError::NoLoadableEntries);
    }
    Ok(programs)
}

/// Validate the fixed header and return the used-entry count.
fn validate_header(bytes: &[u8]) -> Result<usize, T64ParseError> {
    if bytes.len() < HEADER_LEN {
        return Err(T64ParseError::TooShort {
            expected: HEADER_LEN,
            actual: bytes.len(),
        });
    }
    if !bytes[..32].starts_with(b"C64 tape image file") {
        return Err(T64ParseError::InvalidSignature);
    }
    Ok(u16::from_le_bytes([bytes[0x24], bytes[0x25]]) as usize)
}

/// Parse one directory entry at `base`. Returns `Ok(None)` for a non-program
/// (type != 1) entry, `Ok(Some(_))` for a loadable one, or an error if the
/// entry is malformed.
fn parse_entry(bytes: &[u8], base: usize) -> Result<Option<T64Program>, T64ParseError> {
    if bytes[base] != 1 {
        return Ok(None);
    }

    let start_address = u16::from_le_bytes([bytes[base + 2], bytes[base + 3]]);
    let end_address = u16::from_le_bytes([bytes[base + 4], bytes[base + 5]]);
    if end_address <= start_address {
        return Err(T64ParseError::InvalidAddressRange {
            start: start_address,
            end: end_address,
        });
    }

    let data_offset = u32::from_le_bytes([
        bytes[base + 8],
        bytes[base + 9],
        bytes[base + 10],
        bytes[base + 11],
    ]) as usize;
    let data_len = usize::from(end_address - start_address);
    let data_end = data_offset
        .checked_add(data_len)
        .ok_or(T64ParseError::TruncatedEntryData)?;
    if data_end > bytes.len() {
        return Err(T64ParseError::TruncatedEntryData);
    }

    let name = decode_name(&bytes[base + 0x10..base + 0x20]);
    Ok(Some(T64Program {
        name,
        start_address,
        end_address,
        data: bytes[data_offset..data_end].to_vec(),
    }))
}

fn decode_name(bytes: &[u8]) -> String {
    let mut text = String::new();
    for &byte in bytes {
        let ch = match byte {
            0x00 | 0x20 => ' ',
            0x41..=0x5A | 0x30..=0x39 => char::from(byte),
            0xC1..=0xDA => char::from(byte - 0x80),
            _ => '?',
        };
        text.push(ch);
    }
    text.trim_end().to_owned()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn make_t64(name: &[u8], start: u16, data: &[u8]) -> Vec<u8> {
        let mut bytes = vec![0; 0x400];
        bytes[..19].copy_from_slice(b"C64 tape image file");
        bytes[0x22..0x24].copy_from_slice(&(1u16).to_le_bytes());
        bytes[0x24..0x26].copy_from_slice(&(1u16).to_le_bytes());
        bytes[0x40] = 1;
        bytes[0x41] = 0x82;
        bytes[0x42..0x44].copy_from_slice(&start.to_le_bytes());
        bytes[0x44..0x46].copy_from_slice(&(start + data.len() as u16).to_le_bytes());
        bytes[0x48..0x4C].copy_from_slice(&(0x400u32).to_le_bytes());
        bytes[0x50..0x50 + name.len()].copy_from_slice(name);
        bytes.extend_from_slice(data);
        bytes
    }

    #[test]
    fn extracts_first_loadable_program() {
        let image = make_t64(b"HELLO", 0x0801, &[0x11, 0x22, 0x33]);
        let program = extract_first_program(&image).expect("synthetic T64 should parse");

        assert_eq!(program.name, "HELLO");
        assert_eq!(program.start_address, 0x0801);
        assert_eq!(program.end_address, 0x0804);
        assert_eq!(program.data, vec![0x11, 0x22, 0x33]);
        assert_eq!(program.prg_bytes(), vec![0x01, 0x08, 0x11, 0x22, 0x33]);
    }

    /// Build a T64 with two type-1 program entries back to back.
    fn make_t64_two_entries() -> Vec<u8> {
        let mut bytes = vec![0; 0x400];
        bytes[..19].copy_from_slice(b"C64 tape image file");
        bytes[0x22..0x24].copy_from_slice(&(2u16).to_le_bytes()); // max entries
        bytes[0x24..0x26].copy_from_slice(&(2u16).to_le_bytes()); // used entries

        // Entry 0 at $40: "FIRST", $0801, payload [0xAA, 0xBB] at offset $300.
        bytes[0x40] = 1;
        bytes[0x42..0x44].copy_from_slice(&0x0801u16.to_le_bytes());
        bytes[0x44..0x46].copy_from_slice(&0x0803u16.to_le_bytes());
        bytes[0x48..0x4C].copy_from_slice(&0x300u32.to_le_bytes());
        bytes[0x50..0x55].copy_from_slice(b"FIRST");

        // Entry 1 at $60: "SECOND", $C000, payload [0xCC] at offset $302.
        bytes[0x60] = 1;
        bytes[0x62..0x64].copy_from_slice(&0xC000u16.to_le_bytes());
        bytes[0x64..0x66].copy_from_slice(&0xC001u16.to_le_bytes());
        bytes[0x68..0x6C].copy_from_slice(&0x302u32.to_le_bytes());
        bytes[0x70..0x76].copy_from_slice(b"SECOND");

        bytes[0x300] = 0xAA;
        bytes[0x301] = 0xBB;
        bytes[0x302] = 0xCC;
        bytes
    }

    #[test]
    fn extracts_all_program_entries() {
        let image = make_t64_two_entries();
        let programs = extract_programs(&image).expect("two-entry T64 should parse");
        assert_eq!(programs.len(), 2);
        assert_eq!(programs[0].name, "FIRST");
        assert_eq!(programs[0].data, vec![0xAA, 0xBB]);
        assert_eq!(programs[1].name, "SECOND");
        assert_eq!(programs[1].start_address, 0xC000);
        assert_eq!(programs[1].data, vec![0xCC]);
        // The first-entry helper still returns the first of the set.
        assert_eq!(
            extract_first_program(&image).expect("first entry").name,
            "FIRST"
        );
    }

    #[test]
    fn extract_programs_rejects_empty_container() {
        let mut image = vec![0; HEADER_LEN];
        image[..19].copy_from_slice(b"C64 tape image file");
        assert_eq!(
            extract_programs(&image).expect_err("no entries"),
            T64ParseError::NoLoadableEntries
        );
    }

    #[test]
    fn rejects_bad_signature() {
        let err = extract_first_program(&[0; HEADER_LEN]).expect_err("bad signature must fail");
        assert_eq!(err, T64ParseError::InvalidSignature);
    }

    #[test]
    fn rejects_missing_entries() {
        let mut image = vec![0; HEADER_LEN];
        image[..19].copy_from_slice(b"C64 tape image file");
        let err = extract_first_program(&image).expect_err("empty image must fail");
        assert_eq!(err, T64ParseError::NoLoadableEntries);
    }
}
