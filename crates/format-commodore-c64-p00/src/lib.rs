//! Commodore 64 `P00` (PC64) container parsing.
//!
//! `P00` is the PC64 single-file container — the format tools like `Star
//! Commander` produce when copying one file off a 1541 disk to a PC. The name
//! comes from the extension convention: `.P00`, `.P01`, … for PRG files, with
//! `.S00`/`.U00`/`.R00`/`.D00` for SEQ/USR/REL/DEL. All share the same 26-byte
//! header followed by the raw file body.
//!
//! For a PRG the body is a standard C64 program image (2-byte little-endian
//! load address + data), so parsing is just: validate the `C64File` signature,
//! recover the original filename, and hand back the body as PRG bytes.

use thiserror::Error;

/// Fixed PC64 header length: 8-byte magic + 16-byte name + pad + REL record size.
const HEADER_LEN: usize = 0x1A;
const MAGIC: &[u8; 8] = b"C64File\0";

/// A parsed `P00` container.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct P00File {
    /// The original C64 filename recorded in the header (PETSCII, trimmed of
    /// the `$00` padding). Purely informational — the load address comes from
    /// the PRG body, not this name.
    pub name: String,
    /// REL-file record size (`0` for the PRG/SEQ/USR files we load).
    pub record_size: u8,
    /// The file body: for a PRG, a standard load-address-prefixed image.
    pub data: Vec<u8>,
}

impl P00File {
    /// Returns the body as a C64 PRG byte stream (load address + data).
    ///
    /// The body already carries the 2-byte load-address prefix, so this is the
    /// body verbatim; the method exists to mirror the other container crates
    /// (`T64Program::prg_bytes`) at the call sites that load programs.
    #[must_use]
    pub fn prg_bytes(&self) -> Vec<u8> {
        self.data.clone()
    }
}

/// Error surfaced while parsing one `P00` container.
#[derive(Debug, Error, PartialEq, Eq)]
pub enum P00ParseError {
    /// The container is shorter than the fixed 26-byte header.
    #[error("P00 image is too short: expected at least {expected} bytes, got {actual}")]
    TooShort { expected: usize, actual: usize },
    /// The 8-byte signature is not `C64File\0`.
    #[error("P00 image has an invalid header signature")]
    InvalidSignature,
    /// The body is too short to be a PRG (needs at least the load-address word).
    #[error("P00 body is too short to hold a PRG load address")]
    NoLoadAddress,
}

/// Parse a `P00` container.
///
/// # Errors
///
/// Returns an error if the image is shorter than the header, the signature is
/// wrong, or the body is too short to carry a PRG load address.
pub fn parse(bytes: &[u8]) -> Result<P00File, P00ParseError> {
    if bytes.len() < HEADER_LEN {
        return Err(P00ParseError::TooShort {
            expected: HEADER_LEN,
            actual: bytes.len(),
        });
    }
    if &bytes[0..8] != MAGIC {
        return Err(P00ParseError::InvalidSignature);
    }

    // Bytes 8..24: 16-byte PETSCII filename, $00-padded. Byte 24 is unused,
    // byte 25 the REL record size.
    let name = String::from_utf8_lossy(&bytes[8..24])
        .trim_end_matches('\0')
        .trim_end()
        .to_string();
    let record_size = bytes[25];
    let data = bytes[HEADER_LEN..].to_vec();

    if data.len() < 2 {
        return Err(P00ParseError::NoLoadAddress);
    }

    Ok(P00File {
        name,
        record_size,
        data,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Build a minimal P00 image: header + PRG body.
    fn build_p00(name: &[u8], record_size: u8, body: &[u8]) -> Vec<u8> {
        let mut v = Vec::new();
        v.extend_from_slice(MAGIC);
        let mut name_field = [0u8; 16];
        let n = name.len().min(16);
        name_field[..n].copy_from_slice(&name[..n]);
        v.extend_from_slice(&name_field);
        v.push(0x00); // unused
        v.push(record_size);
        v.extend_from_slice(body);
        v
    }

    #[test]
    fn parses_prg_body_and_name() {
        // Load address $0801 (BASIC start) + a few payload bytes.
        let body = [0x01, 0x08, 0xAA, 0xBB, 0xCC];
        let img = build_p00(b"MYGAME", 0, &body);
        let file = parse(&img).expect("valid P00");
        assert_eq!(file.name, "MYGAME");
        assert_eq!(file.record_size, 0);
        assert_eq!(file.prg_bytes(), body);
        // Load address preserved at the front of the PRG bytes.
        assert_eq!(&file.prg_bytes()[0..2], &[0x01, 0x08]);
    }

    #[test]
    fn rejects_bad_signature() {
        let mut img = build_p00(b"X", 0, &[0x01, 0x08, 0x00]);
        img[0] = b'Z';
        assert_eq!(parse(&img), Err(P00ParseError::InvalidSignature));
    }

    #[test]
    fn rejects_truncated_header() {
        assert!(matches!(
            parse(&[0u8; 8]),
            Err(P00ParseError::TooShort { .. })
        ));
    }

    #[test]
    fn rejects_body_without_load_address() {
        let img = build_p00(b"TINY", 0, &[0x01]);
        assert_eq!(parse(&img), Err(P00ParseError::NoLoadAddress));
    }
}
