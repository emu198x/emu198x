//! DragonDOS `.BIN` machine-code program parser.
//!
//! XRoar identifies this format as "DragonDOS binary". The archived Dragon
//! `[BIN]` game fixtures use the same framing:
//!
//! `0x55 <file_type> <load_be> <length_be> <exec_be> 0xaa <payload>`
//!
//! The known Dragon machine-code files use file type `0x02`, matching the CAS
//! namefile machine-code type.

use serde::{Deserialize, Serialize};
use thiserror::Error;

/// DragonDOS `.BIN` sentinel byte.
pub const DRAGON_BIN_SENTINEL: u8 = 0x55;

/// Machine-code file type used by DragonDOS `.BIN` files.
pub const DRAGON_BIN_MACHINE_CODE_TYPE: u8 = 0x02;

/// DragonDOS `.BIN` separator byte between the header and payload.
pub const DRAGON_BIN_PAYLOAD_SEPARATOR: u8 = 0xaa;

const DRAGON_BIN_HEADER_LEN: usize = 9;

/// Parsed DragonDOS binary program.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct DragonBinImage {
    /// Raw DragonDOS file type byte.
    pub file_type: u8,
    /// Target load address.
    pub load_address: u16,
    /// Execution address.
    pub exec_address: u16,
    /// Program bytes to copy to RAM at [`Self::load_address`].
    pub payload: Vec<u8>,
}

impl DragonBinImage {
    /// Last inclusive address written by this image.
    #[must_use]
    pub fn end_address(&self) -> Option<u16> {
        if self.payload.is_empty() {
            return None;
        }
        let len = u16::try_from(self.payload.len()).ok()?;
        Some(self.load_address.wrapping_add(len).wrapping_sub(1))
    }
}

/// DragonDOS `.BIN` parse failure.
#[derive(Clone, Debug, Error, PartialEq, Eq)]
pub enum DragonBinParseError {
    /// The image is too short to contain a DragonDOS `.BIN` header.
    #[error("DragonDOS BIN is too short: got {actual} bytes, need at least {minimum}")]
    TooShort { actual: usize, minimum: usize },

    /// The first byte does not identify a DragonDOS `.BIN` image.
    #[error("DragonDOS BIN has sentinel 0x{actual:02X}, expected 0x55")]
    BadSentinel { actual: u8 },

    /// The file type is not a supported Dragon machine-code binary.
    #[error("unsupported DragonDOS BIN file type 0x{actual:02X}")]
    UnsupportedFileType { actual: u8 },

    /// The byte between header and payload is not the expected separator.
    #[error("DragonDOS BIN has payload separator 0x{actual:02X}, expected 0xAA")]
    BadPayloadSeparator { actual: u8 },

    /// The declared payload length does not match the remaining image length.
    #[error(
        "DragonDOS BIN declares {declared} payload bytes, but {actual} bytes remain after the header"
    )]
    LengthMismatch { declared: usize, actual: usize },
}

/// Parses one DragonDOS `.BIN` machine-code program.
///
/// # Errors
///
/// Returns an error when the header sentinel, file type, separator, or declared
/// payload length does not match the supported DragonDOS binary format.
pub fn parse_dragon_bin(bytes: &[u8]) -> Result<DragonBinImage, DragonBinParseError> {
    if bytes.len() < DRAGON_BIN_HEADER_LEN {
        return Err(DragonBinParseError::TooShort {
            actual: bytes.len(),
            minimum: DRAGON_BIN_HEADER_LEN,
        });
    }

    if bytes[0] != DRAGON_BIN_SENTINEL {
        return Err(DragonBinParseError::BadSentinel { actual: bytes[0] });
    }

    let file_type = bytes[1];
    if file_type != DRAGON_BIN_MACHINE_CODE_TYPE {
        return Err(DragonBinParseError::UnsupportedFileType { actual: file_type });
    }

    let load_address = u16::from_be_bytes([bytes[2], bytes[3]]);
    let declared_len = usize::from(u16::from_be_bytes([bytes[4], bytes[5]]));
    let exec_address = u16::from_be_bytes([bytes[6], bytes[7]]);

    if bytes[8] != DRAGON_BIN_PAYLOAD_SEPARATOR {
        return Err(DragonBinParseError::BadPayloadSeparator { actual: bytes[8] });
    }

    let payload = &bytes[DRAGON_BIN_HEADER_LEN..];
    if payload.len() != declared_len {
        return Err(DragonBinParseError::LengthMismatch {
            declared: declared_len,
            actual: payload.len(),
        });
    }

    Ok(DragonBinImage {
        file_type,
        load_address,
        exec_address,
        payload: payload.to_vec(),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_dragon_dos_binary_header_and_payload() {
        let image = parse_dragon_bin(&[
            0x55, 0x02, 0x28, 0x00, 0x00, 0x03, 0x28, 0x10, 0xaa, 0xcc, 0xfc, 0x39,
        ])
        .expect("valid DragonDOS BIN should parse");

        assert_eq!(image.file_type, DRAGON_BIN_MACHINE_CODE_TYPE);
        assert_eq!(image.load_address, 0x2800);
        assert_eq!(image.exec_address, 0x2810);
        assert_eq!(image.end_address(), Some(0x2802));
        assert_eq!(image.payload, vec![0xcc, 0xfc, 0x39]);
    }

    #[test]
    fn rejects_non_dragon_dos_binary_sentinel() {
        let err =
            parse_dragon_bin(&[0x00; DRAGON_BIN_HEADER_LEN]).expect_err("wrong sentinel must fail");
        assert_eq!(err, DragonBinParseError::BadSentinel { actual: 0x00 });
    }

    #[test]
    fn rejects_unsupported_file_type() {
        let err = parse_dragon_bin(&[0x55, 0x00, 0, 0, 0, 0, 0, 0, 0xaa])
            .expect_err("non-machine-code file type must fail");
        assert_eq!(
            err,
            DragonBinParseError::UnsupportedFileType { actual: 0x00 }
        );
    }

    #[test]
    fn rejects_length_mismatch() {
        let err = parse_dragon_bin(&[0x55, 0x02, 0, 0, 0, 2, 0, 0, 0xaa, 0x39])
            .expect_err("declared length mismatch must fail");
        assert_eq!(
            err,
            DragonBinParseError::LengthMismatch {
                declared: 2,
                actual: 1,
            }
        );
    }
}
