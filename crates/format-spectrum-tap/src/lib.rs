//! ZX Spectrum TAP tape image format.
//!
//! TAP is the simplest Spectrum tape format: sequential blocks of data,
//! each preceded by a 2-byte little-endian length word. Each block contains
//! a flag byte, data bytes, and a checksum byte (XOR of flag + data).
//!
//! A typical program consists of two blocks:
//!   1. Header block (flag $00, 17 bytes of metadata)
//!   2. Data block (flag $FF, the actual program/data)

/// Header type byte in a standard Spectrum TAP header.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum HeaderType {
    /// BASIC program (type 0).
    Program = 0,
    /// Number array (type 1).
    NumberArray = 1,
    /// Character array (type 2).
    CharacterArray = 2,
    /// Code block / raw bytes (type 3).
    Code = 3,
}

/// A single block from a TAP file.
#[derive(Debug, Clone)]
pub struct TapBlock {
    /// Flag byte: $00 = header, $FF = data.
    pub flag: u8,
    /// Block data (excludes the flag and checksum bytes).
    pub data: Vec<u8>,
}

impl TapBlock {
    /// Create a new block with the given flag and data.
    #[must_use]
    pub fn new(flag: u8, data: Vec<u8>) -> Self {
        Self { flag, data }
    }

    /// Create a standard 17-byte header block.
    ///
    /// `name` is padded or truncated to exactly 10 characters.
    /// `data_length` is the length of the corresponding data block.
    /// `param1` and `param2` are type-specific (e.g. autorun line and program length).
    #[must_use]
    pub fn header(
        header_type: HeaderType,
        name: &str,
        data_length: u16,
        param1: u16,
        param2: u16,
    ) -> Self {
        let mut data = vec![header_type as u8];

        // Filename: exactly 10 bytes, padded with spaces
        let name_bytes = name.as_bytes();
        for i in 0..10 {
            data.push(if i < name_bytes.len() {
                name_bytes[i]
            } else {
                b' '
            });
        }

        // Data length (little-endian)
        data.push(data_length as u8);
        data.push((data_length >> 8) as u8);

        // Param 1 (little-endian) — for Program: autorun line (>=32768 means no autorun)
        data.push(param1 as u8);
        data.push((param1 >> 8) as u8);

        // Param 2 (little-endian) — for Program: offset to variable area
        data.push(param2 as u8);
        data.push((param2 >> 8) as u8);

        Self { flag: 0x00, data }
    }

    /// Create a header block for a BASIC program.
    ///
    /// `autorun_line` of `None` means no autorun.
    /// `program_length` is the length of the tokenised program (without variables).
    #[must_use]
    pub fn program_header(
        name: &str,
        data_length: u16,
        autorun_line: Option<u16>,
        program_length: u16,
    ) -> Self {
        Self::header(
            HeaderType::Program,
            name,
            data_length,
            autorun_line.unwrap_or(0x8000),
            program_length,
        )
    }

    /// Create a data block (flag $FF) with the given payload.
    #[must_use]
    pub fn data(payload: Vec<u8>) -> Self {
        Self {
            flag: 0xFF,
            data: payload,
        }
    }

    /// Serialise this block to TAP format (length word + flag + data + checksum).
    #[must_use]
    pub fn to_bytes(&self) -> Vec<u8> {
        let mut checksum = self.flag;
        for &b in &self.data {
            checksum ^= b;
        }

        let len = (self.data.len() + 2) as u16; // flag + data + checksum
        let mut out = Vec::with_capacity(len as usize + 2);
        out.push(len as u8);
        out.push((len >> 8) as u8);
        out.push(self.flag);
        out.extend_from_slice(&self.data);
        out.push(checksum);
        out
    }
}

/// A parsed TAP file containing sequential blocks.
#[derive(Debug, Clone)]
pub struct TapFile {
    /// The blocks in the TAP file, in order.
    pub blocks: Vec<TapBlock>,
}

impl TapFile {
    /// Create an empty TAP file.
    #[must_use]
    pub fn new() -> Self {
        Self { blocks: Vec::new() }
    }

    /// Serialise all blocks to TAP format.
    #[must_use]
    pub fn to_bytes(&self) -> Vec<u8> {
        let mut out = Vec::new();
        for block in &self.blocks {
            out.extend(block.to_bytes());
        }
        out
    }

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

impl Default for TapFile {
    fn default() -> Self {
        Self::new()
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

    #[test]
    fn roundtrip_single_block() {
        let block = TapBlock::new(0xFF, vec![1, 2, 3, 4, 5]);
        let bytes = block.to_bytes();
        let parsed = TapFile::parse(&bytes).expect("roundtrip should parse");
        assert_eq!(parsed.blocks.len(), 1);
        assert_eq!(parsed.blocks[0].flag, 0xFF);
        assert_eq!(parsed.blocks[0].data, &[1, 2, 3, 4, 5]);
    }

    #[test]
    fn roundtrip_program_header_and_data() {
        let program_data = vec![0xEF, 0x22, 0x48, 0x45, 0x4C, 0x4C, 0x4F, 0x22, 0x0D];
        let header = TapBlock::program_header("test", program_data.len() as u16, Some(10), program_data.len() as u16);
        let data = TapBlock::data(program_data.clone());

        let mut tap = TapFile::new();
        tap.blocks.push(header);
        tap.blocks.push(data);

        let bytes = tap.to_bytes();
        let parsed = TapFile::parse(&bytes).expect("program roundtrip should parse");
        assert_eq!(parsed.blocks.len(), 2);
        assert_eq!(parsed.blocks[0].flag, 0x00);
        assert_eq!(parsed.blocks[0].data.len(), 17);
        assert_eq!(parsed.blocks[0].data[0], 0x00); // Program type
        assert_eq!(parsed.blocks[1].flag, 0xFF);
        assert_eq!(parsed.blocks[1].data, program_data);
    }

    #[test]
    fn program_header_name_padding() {
        let header = TapBlock::program_header("Hi", 100, None, 100);
        // Name occupies bytes 1..11 of the header data
        let name_bytes = &header.data[1..11];
        assert_eq!(name_bytes, b"Hi        ");
    }

    #[test]
    fn program_header_name_truncation() {
        let header = TapBlock::program_header("Very Long Name", 100, None, 100);
        let name_bytes = &header.data[1..11];
        assert_eq!(name_bytes, b"Very Long ");
    }
}
