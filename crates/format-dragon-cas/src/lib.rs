//! Dragon 32 CAS cassette image parser.
//!
//! CAS files store the byte stream that the ROM cassette routines see, not the
//! analogue waveform. Standard blocks are encoded as:
//!
//! `0x55... 0x3c <type> <length> <payload> <checksum>`
//!
//! The checksum is the wrapping sum of the type byte, length byte, and payload.
//! Timing reconstruction belongs in the machine cassette peripheral; this crate
//! only parses and validates the byte container.

use thiserror::Error;

/// Leader byte written before cassette blocks.
pub const LEADER_BYTE: u8 = 0x55;

/// Sync byte that introduces a cassette block.
pub const SYNC_BYTE: u8 = 0x3c;

/// Parsed Dragon CAS image.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CasImage {
    /// Blocks in tape order.
    pub blocks: Vec<CasBlock>,
}

impl CasImage {
    /// Returns the first standard namefile header, if present.
    #[must_use]
    pub fn first_header(&self) -> Option<&CasHeader> {
        self.blocks.iter().find_map(|block| block.header.as_ref())
    }

    /// Returns `true` when every parsed block checksum matches.
    #[must_use]
    pub fn checksums_valid(&self) -> bool {
        self.blocks.iter().all(|block| block.checksum_valid)
    }
}

/// One CAS block.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CasBlock {
    /// Offset of the block leader, or the sync byte when no leader was present.
    pub offset: usize,
    /// Offset of the sync byte.
    pub sync_offset: usize,
    /// Number of `0x55` leader bytes immediately before this block.
    pub leader_len: usize,
    /// Decoded block kind.
    pub kind: CasBlockKind,
    /// Raw block type byte.
    pub block_type: u8,
    /// Block payload, excluding framing and checksum.
    pub data: Vec<u8>,
    /// Checksum byte stored in the CAS image.
    pub checksum: u8,
    /// Checksum calculated from the block type, length, and payload.
    pub calculated_checksum: u8,
    /// Whether `checksum` equals `calculated_checksum`.
    pub checksum_valid: bool,
    /// Decoded namefile header for standard type-0, length-15 blocks.
    pub header: Option<CasHeader>,
}

/// CAS block type.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CasBlockKind {
    /// Standard namefile header block.
    Header,
    /// Data payload block.
    Data,
    /// End-of-file marker.
    EndOfFile,
    /// Unknown or non-standard block type.
    Unknown(u8),
}

impl CasBlockKind {
    const fn from_byte(value: u8) -> Self {
        match value {
            0x00 => Self::Header,
            0x01 => Self::Data,
            0xff => Self::EndOfFile,
            other => Self::Unknown(other),
        }
    }
}

/// Standard 15-byte namefile header payload.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CasHeader {
    /// Raw eight-byte filename field.
    pub raw_name: [u8; 8],
    /// Filename with trailing NULs and spaces removed.
    pub name: String,
    /// File type declared by the header.
    pub file_type: CasFileType,
    /// Raw ASCII/binary flag byte.
    pub ascii_flag: u8,
    /// Raw continuous-gap flag byte.
    pub gap_flag: u8,
    /// First big-endian address field. Loader meaning depends on file type.
    pub first_address: u16,
    /// Second big-endian address field. Loader meaning depends on file type.
    pub second_address: u16,
}

impl CasHeader {
    fn from_payload(payload: &[u8]) -> Self {
        let mut raw_name = [0; 8];
        raw_name.copy_from_slice(&payload[..8]);

        let name = String::from_utf8_lossy(&raw_name)
            .trim_end_matches(['\0', ' '])
            .to_string();

        Self {
            raw_name,
            name,
            file_type: CasFileType::from_byte(payload[8]),
            ascii_flag: payload[9],
            gap_flag: payload[10],
            first_address: u16::from_be_bytes([payload[11], payload[12]]),
            second_address: u16::from_be_bytes([payload[13], payload[14]]),
        }
    }
}

/// File type stored in a namefile header.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CasFileType {
    /// Tokenized BASIC program.
    Basic,
    /// Data file.
    Data,
    /// Machine-code program.
    MachineCode,
    /// Unknown or non-standard file type.
    Unknown(u8),
}

impl CasFileType {
    const fn from_byte(value: u8) -> Self {
        match value {
            0x00 => Self::Basic,
            0x01 => Self::Data,
            0x02 => Self::MachineCode,
            other => Self::Unknown(other),
        }
    }
}

/// CAS parse failure.
#[derive(Clone, Debug, Error, PartialEq, Eq)]
pub enum CasParseError {
    /// A non-leader, non-sync byte appeared where a block should start.
    #[error("unexpected byte 0x{actual:02X} at offset {offset}; expected CAS sync byte 0x3C")]
    UnexpectedByte { offset: usize, actual: u8 },

    /// The image ended before a block type and length could be read.
    #[error("CAS block at offset {offset} has a truncated header")]
    TruncatedBlockHeader { offset: usize },

    /// The image ended before the declared payload and checksum were available.
    #[error(
        "CAS block at offset {offset} declares {declared} payload bytes, but only {available} bytes plus checksum are available"
    )]
    TruncatedBlockPayload {
        offset: usize,
        declared: usize,
        available: usize,
    },
}

/// Parses a Dragon CAS image into framed blocks.
///
/// # Errors
///
/// Returns an error when a block is truncated or unexpected bytes appear
/// outside block framing.
pub fn parse_cas(bytes: &[u8]) -> Result<CasImage, CasParseError> {
    let mut blocks = Vec::new();
    let mut pos = 0usize;

    while pos < bytes.len() {
        let offset = pos;
        let mut leader_len = 0usize;
        while pos < bytes.len() && bytes[pos] == LEADER_BYTE {
            leader_len += 1;
            pos += 1;
        }

        if pos == bytes.len() {
            break;
        }

        if bytes[pos] != SYNC_BYTE {
            return Err(CasParseError::UnexpectedByte {
                offset: pos,
                actual: bytes[pos],
            });
        }

        let sync_offset = pos;
        pos += 1;

        if pos + 2 > bytes.len() {
            return Err(CasParseError::TruncatedBlockHeader {
                offset: sync_offset,
            });
        }

        let block_type = bytes[pos];
        let length = bytes[pos + 1];
        let payload_len = usize::from(length);
        pos += 2;

        if pos + payload_len + 1 > bytes.len() {
            return Err(CasParseError::TruncatedBlockPayload {
                offset: sync_offset,
                declared: payload_len,
                available: bytes.len().saturating_sub(pos),
            });
        }

        let data = bytes[pos..pos + payload_len].to_vec();
        pos += payload_len;

        let checksum = bytes[pos];
        pos += 1;
        let calculated_checksum = checksum_for(block_type, length, &data);
        let header = if matches!(block_type, 0x00) && data.len() == 15 {
            Some(CasHeader::from_payload(&data))
        } else {
            None
        };

        blocks.push(CasBlock {
            offset,
            sync_offset,
            leader_len,
            kind: CasBlockKind::from_byte(block_type),
            block_type,
            data,
            checksum,
            calculated_checksum,
            checksum_valid: checksum == calculated_checksum,
            header,
        });
    }

    Ok(CasImage { blocks })
}

/// Computes a CAS block checksum.
#[must_use]
pub fn checksum_for(block_type: u8, length: u8, payload: &[u8]) -> u8 {
    payload
        .iter()
        .fold(block_type.wrapping_add(length), |sum, byte| {
            sum.wrapping_add(*byte)
        })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn block(block_type: u8, payload: &[u8]) -> Vec<u8> {
        let length = payload.len() as u8;
        let mut bytes = vec![LEADER_BYTE, LEADER_BYTE, SYNC_BYTE, block_type, length];
        bytes.extend_from_slice(payload);
        bytes.push(checksum_for(block_type, length, payload));
        bytes
    }

    #[test]
    fn parses_standard_header_data_and_eof_blocks() {
        let header_payload = [
            b'T', b'E', b'S', b'T', b' ', b' ', b' ', b' ', 0x02, 0x00, 0x00, 0x12, 0x34, 0x56,
            0x78,
        ];
        let mut cas = block(0x00, &header_payload);
        cas.extend(block(0x01, &[0xaa, 0xbb, 0xcc]));
        cas.extend(block(0xff, &[]));

        let image = parse_cas(&cas).expect("CAS should parse");

        assert_eq!(image.blocks.len(), 3);
        assert!(image.checksums_valid());
        assert_eq!(image.blocks[0].kind, CasBlockKind::Header);
        assert_eq!(image.blocks[1].kind, CasBlockKind::Data);
        assert_eq!(image.blocks[2].kind, CasBlockKind::EndOfFile);

        let header = image.first_header().expect("header should decode");
        assert_eq!(header.name, "TEST");
        assert_eq!(header.file_type, CasFileType::MachineCode);
        assert_eq!(header.first_address, 0x1234);
        assert_eq!(header.second_address, 0x5678);
    }

    #[test]
    fn parses_real_textstar_header_prefix() {
        let cas = [
            0x55, 0x55, 0x55, 0x55, 0x55, 0x55, 0x55, 0x55, 0x55, 0x55, 0x55, 0x55, 0x55, 0x55,
            0x55, 0x55, 0x3c, 0x00, 0x0f, 0x54, 0x45, 0x58, 0x54, 0x53, 0x54, 0x41, 0x52, 0x00,
            0x00, 0x00, 0x41, 0x43, 0x45, 0x20, 0x77, 0x55, 0x3c, 0xff, 0x00, 0xff,
        ];

        let image = parse_cas(&cas).expect("real CAS prefix should parse");

        assert_eq!(image.blocks.len(), 2);
        assert_eq!(image.blocks[0].leader_len, 16);
        assert_eq!(image.blocks[0].calculated_checksum, 0x77);
        let header = image.first_header().expect("header should decode");
        assert_eq!(header.name, "TEXTSTAR");
        assert_eq!(header.file_type, CasFileType::Basic);
    }

    #[test]
    fn accepts_missing_leader_and_trailing_leader() {
        let mut cas = vec![SYNC_BYTE, 0xff, 0x00, 0xff, LEADER_BYTE];

        let image = parse_cas(&cas).expect("CAS should parse without leader");
        assert_eq!(image.blocks.len(), 1);
        assert_eq!(image.blocks[0].leader_len, 0);

        cas.push(LEADER_BYTE);
        let image = parse_cas(&cas).expect("trailing leader should be ignored");
        assert_eq!(image.blocks.len(), 1);
    }

    #[test]
    fn exposes_bad_checksum_without_rejecting_the_image() {
        let mut cas = block(0x01, &[0x01, 0x02]);
        let last = cas.last_mut().expect("test block has checksum");
        *last = last.wrapping_add(1);

        let image = parse_cas(&cas).expect("CAS with bad checksum should still frame");

        assert!(!image.checksums_valid());
        assert!(!image.blocks[0].checksum_valid);
    }

    #[test]
    fn rejects_unexpected_bytes_between_blocks() {
        let err = parse_cas(&[0x00]).expect_err("unexpected byte should fail");

        assert_eq!(
            err,
            CasParseError::UnexpectedByte {
                offset: 0,
                actual: 0x00,
            }
        );
    }

    #[test]
    fn rejects_truncated_block_header() {
        let err = parse_cas(&[SYNC_BYTE, 0x01]).expect_err("truncated header should fail");

        assert_eq!(err, CasParseError::TruncatedBlockHeader { offset: 0 });
    }

    #[test]
    fn rejects_truncated_block_payload() {
        let err =
            parse_cas(&[SYNC_BYTE, 0x01, 0x02, 0xaa]).expect_err("truncated payload should fail");

        assert_eq!(
            err,
            CasParseError::TruncatedBlockPayload {
                offset: 0,
                declared: 2,
                available: 1,
            }
        );
    }
}
