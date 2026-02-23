//! TAP file format parser.
//!
//! TAP is the simplest Spectrum tape format: sequential blocks of data,
//! each preceded by a 2-byte little-endian length word. Each block contains
//! a flag byte, data bytes, and a checksum byte (XOR of flag + data).
//!
//! A typical program consists of two blocks:
//!   1. Header block (flag $00, 17 bytes of metadata)
//!   2. Data block (flag $FF, the actual program/data)

/// A single block from a TAP file.
#[derive(Debug, Clone)]
pub struct TapBlock {
    /// Flag byte: $00 = header, $FF = data.
    pub flag: u8,
    /// Block data (excludes the flag and checksum bytes).
    pub data: Vec<u8>,
}

/// A parsed TAP file containing sequential blocks.
#[derive(Debug, Clone)]
pub struct TapFile {
    /// The blocks in the TAP file, in order.
    pub blocks: Vec<TapBlock>,
}

impl TapFile {
    /// Parse a TAP file from raw bytes.
    ///
    /// # Errors
    ///
    /// Returns an error if the data is malformed (truncated block, bad length).
    pub fn parse(data: &[u8]) -> Result<Self, String> {
        let mut blocks = Vec::new();
        let mut offset = 0;

        while offset < data.len() {
            // Need at least 2 bytes for the block length
            if offset + 2 > data.len() {
                return Err(format!(
                    "Truncated TAP file: expected 2-byte length at offset {offset}"
                ));
            }

            let block_len = u16::from(data[offset]) | (u16::from(data[offset + 1]) << 8);
            offset += 2;

            let block_len = block_len as usize;
            if block_len < 2 {
                return Err(format!(
                    "TAP block at offset {} has length {block_len}, minimum is 2 (flag + checksum)",
                    offset - 2
                ));
            }

            if offset + block_len > data.len() {
                return Err(format!(
                    "Truncated TAP block at offset {}: need {block_len} bytes, only {} remain",
                    offset - 2,
                    data.len() - offset
                ));
            }

            let flag = data[offset];
            let checksum = data[offset + block_len - 1];
            let block_data = &data[offset + 1..offset + block_len - 1];

            // Verify checksum: XOR of flag + all data bytes
            let mut expected = flag;
            for &byte in block_data {
                expected ^= byte;
            }
            if expected != checksum {
                return Err(format!(
                    "TAP block at offset {}: checksum mismatch (expected ${expected:02X}, got ${checksum:02X})",
                    offset - 2
                ));
            }

            blocks.push(TapBlock {
                flag,
                data: block_data.to_vec(),
            });

            offset += block_len;
        }

        Ok(Self { blocks })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Build a TAP block from flag + data, computing the length word and checksum.
    fn make_tap_block(flag: u8, data: &[u8]) -> Vec<u8> {
        let mut checksum = flag;
        for &b in data {
            checksum ^= b;
        }
        // Length = flag + data + checksum
        let len = (data.len() + 2) as u16;
        let mut block = Vec::new();
        block.push(len as u8);
        block.push((len >> 8) as u8);
        block.push(flag);
        block.extend_from_slice(data);
        block.push(checksum);
        block
    }

    #[test]
    fn parse_empty_file() {
        let tap = TapFile::parse(&[]).expect("empty file is valid");
        assert!(tap.blocks.is_empty());
    }

    #[test]
    fn parse_single_block() {
        let block = make_tap_block(0x00, &[1, 2, 3, 4, 5]);
        let tap = TapFile::parse(&block).expect("single block should parse");
        assert_eq!(tap.blocks.len(), 1);
        assert_eq!(tap.blocks[0].flag, 0x00);
        assert_eq!(tap.blocks[0].data, &[1, 2, 3, 4, 5]);
    }

    #[test]
    fn parse_two_blocks() {
        let mut data = make_tap_block(0x00, &[0x11, 0x22]);
        data.extend(make_tap_block(0xFF, &[0xAA, 0xBB, 0xCC]));

        let tap = TapFile::parse(&data).expect("two blocks should parse");
        assert_eq!(tap.blocks.len(), 2);
        assert_eq!(tap.blocks[0].flag, 0x00);
        assert_eq!(tap.blocks[0].data, &[0x11, 0x22]);
        assert_eq!(tap.blocks[1].flag, 0xFF);
        assert_eq!(tap.blocks[1].data, &[0xAA, 0xBB, 0xCC]);
    }

    #[test]
    fn parse_standard_header_block() {
        // Standard header: flag 0x00, 17 bytes of metadata
        let header_data = [0u8; 17]; // All zeros = Program type
        let block = make_tap_block(0x00, &header_data);
        let tap = TapFile::parse(&block).expect("header block should parse");
        assert_eq!(tap.blocks[0].flag, 0x00);
        assert_eq!(tap.blocks[0].data.len(), 17);
    }

    #[test]
    fn parse_truncated_length() {
        assert!(TapFile::parse(&[0x05]).is_err());
    }

    #[test]
    fn parse_truncated_block() {
        // Length says 5 bytes but only 3 available
        assert!(TapFile::parse(&[0x05, 0x00, 0x00, 0x01, 0x02]).is_err());
    }

    #[test]
    fn parse_bad_checksum() {
        let mut block = make_tap_block(0x00, &[1, 2, 3]);
        // Corrupt the checksum (last byte)
        let last = block.len() - 1;
        block[last] ^= 0xFF;
        assert!(TapFile::parse(&block).is_err());
    }

    #[test]
    fn parse_minimum_block() {
        // Minimum valid block: flag + checksum only (no data)
        // flag=0x00, checksum=0x00 (XOR of just the flag)
        let block = [0x02, 0x00, 0x00, 0x00]; // len=2, flag=0, checksum=0
        let tap = TapFile::parse(&block).expect("minimum block should parse");
        assert_eq!(tap.blocks.len(), 1);
        assert_eq!(tap.blocks[0].flag, 0x00);
        assert!(tap.blocks[0].data.is_empty());
    }

    #[test]
    fn parse_too_short_block_length() {
        // Block length 0 is invalid (minimum is 2: flag + checksum)
        assert!(TapFile::parse(&[0x00, 0x00]).is_err());
        // Block length 1 is also invalid
        assert!(TapFile::parse(&[0x01, 0x00, 0xFF]).is_err());
    }
}
