//! Parser for the Acorn Atom `.atm` program image (the Wouter Ras format used by
//! Atomulator and others).
//!
//! An `.atm` file is a direct memory image: a 22-byte header followed by the
//! program body, loaded into RAM at the header's load address.
//!
//! ```text
//! offset  size  field
//! 0       16    filename (ASCII, zero/space padded)
//! 16      2     load address  (little-endian)
//! 18      2     execution address (little-endian)
//! 20      2     length of the body (little-endian)
//! 22      N     body (N = length bytes)
//! ```

use serde::{Deserialize, Serialize};
use thiserror::Error;

/// Fixed `.atm` header size: 16-byte name + three little-endian `u16`s.
const HEADER_LEN: usize = 22;
/// Filename field width.
const NAME_LEN: usize = 16;

/// A parsed `.atm` program image.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct AtmImage {
    /// The stored filename (trailing NUL/space padding trimmed).
    pub filename: String,
    /// Address the body loads to.
    pub load_address: u16,
    /// Address to begin execution at (auto-run / `LINK` target).
    pub exec_address: u16,
    /// The program body.
    pub payload: Vec<u8>,
}

impl AtmImage {
    /// One past the last byte the body occupies in memory.
    #[must_use]
    pub fn end_address(&self) -> usize {
        self.load_address as usize + self.payload.len()
    }
}

/// Errors from [`parse`].
#[derive(Debug, Error, PartialEq, Eq)]
pub enum AtmError {
    /// The image is smaller than the 22-byte header.
    #[error("`.atm` image is {actual} bytes; need at least {HEADER_LEN} for the header")]
    TooShort {
        /// The actual byte count.
        actual: usize,
    },
    /// The declared body length does not match the bytes present.
    #[error("`.atm` declares a {declared}-byte body but {actual} bytes follow the header")]
    LengthMismatch {
        /// The length field's value.
        declared: usize,
        /// Bytes actually present after the header.
        actual: usize,
    },
}

/// Parse an `.atm` program image.
///
/// # Errors
///
/// Returns [`AtmError::TooShort`] if the input is smaller than the header, or
/// [`AtmError::LengthMismatch`] if the declared body length exceeds the bytes
/// available.
pub fn parse(bytes: &[u8]) -> Result<AtmImage, AtmError> {
    if bytes.len() < HEADER_LEN {
        return Err(AtmError::TooShort {
            actual: bytes.len(),
        });
    }
    let filename = bytes[0..NAME_LEN]
        .iter()
        .take_while(|&&b| b != 0)
        .map(|&b| b as char)
        .collect::<String>()
        .trim_end()
        .to_owned();
    let load_address = u16::from_le_bytes([bytes[16], bytes[17]]);
    let exec_address = u16::from_le_bytes([bytes[18], bytes[19]]);
    let declared = usize::from(u16::from_le_bytes([bytes[20], bytes[21]]));
    let body = &bytes[HEADER_LEN..];
    if declared > body.len() {
        return Err(AtmError::LengthMismatch {
            declared,
            actual: body.len(),
        });
    }
    Ok(AtmImage {
        filename,
        load_address,
        exec_address,
        payload: body[..declared].to_vec(),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn image(name: &[u8], load: u16, exec: u16, body: &[u8]) -> Vec<u8> {
        let mut v = vec![0u8; NAME_LEN];
        v[..name.len()].copy_from_slice(name);
        v.extend_from_slice(&load.to_le_bytes());
        v.extend_from_slice(&exec.to_le_bytes());
        v.extend_from_slice(&(body.len() as u16).to_le_bytes());
        v.extend_from_slice(body);
        v
    }

    #[test]
    fn parses_a_valid_image() {
        let raw = image(b"DEFENDER", 0x2900, 0x2900, &[0xA9, 0x42, 0x60]);
        let atm = parse(&raw).expect("valid .atm");
        assert_eq!(atm.filename, "DEFENDER");
        assert_eq!(atm.load_address, 0x2900);
        assert_eq!(atm.exec_address, 0x2900);
        assert_eq!(atm.payload, vec![0xA9, 0x42, 0x60]);
        assert_eq!(atm.end_address(), 0x2903);
    }

    #[test]
    fn rejects_a_short_image() {
        assert_eq!(parse(&[0u8; 10]), Err(AtmError::TooShort { actual: 10 }));
    }

    #[test]
    fn rejects_a_truncated_body() {
        let mut raw = image(b"X", 0x0200, 0x0200, &[1, 2, 3]);
        raw.truncate(HEADER_LEN + 1); // declares 3 body bytes, only 1 present
        assert_eq!(
            parse(&raw),
            Err(AtmError::LengthMismatch {
                declared: 3,
                actual: 1,
            })
        );
    }

    #[test]
    fn trims_name_padding_but_ignores_trailing_garbage_body() {
        // A body longer than `length` is allowed; only `length` bytes are taken.
        let mut raw = image(b"HI", 0x70, 0x70, &[0xEE]);
        raw.push(0xFF); // extra byte beyond the declared length
        let atm = parse(&raw).expect("valid");
        assert_eq!(atm.filename, "HI");
        assert_eq!(atm.payload, vec![0xEE]);
    }
}
