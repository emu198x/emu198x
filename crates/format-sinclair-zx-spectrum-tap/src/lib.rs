//! TAP tape format parser for the Sinclair ZX Spectrum.
//!
//! Source references:
//! - `knowledge/concepts/tape-formats.md`
//! - Adapted from `../Emu198x-Older/crates/format-sinclair-zx-spectrum-tap/src/lib.rs`
//!
//! TAP is a block container, not a timing-preserving format. Each block stores
//! exactly the bytes that the ROM loader would see on tape: one flag byte, a
//! payload, and one checksum byte. Timing is reconstructed later using the
//! standard ROM pulse rules.

/// One raw TAP block.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct TapBlock {
    /// Flag byte (`0x00` for a header, `0xFF` for a data block).
    pub flag: u8,
    /// Block payload, excluding the leading flag byte and trailing checksum.
    pub data: Vec<u8>,
}

impl TapBlock {
    /// Returns `true` when the block is a ROM header block.
    #[must_use]
    pub fn is_header(&self) -> bool {
        self.flag < 0x80
    }
}

/// Parses a TAP file into a sequence of raw blocks.
///
/// # Errors
///
/// Returns an error if a block length overruns the supplied byte slice.
pub fn parse_tap(data: &[u8]) -> Result<Vec<TapBlock>, String> {
    let mut blocks = Vec::new();
    let mut pos = 0usize;

    while pos + 2 <= data.len() {
        let len = usize::from(u16::from_le_bytes([data[pos], data[pos + 1]]));
        pos += 2;

        if len == 0 {
            continue;
        }

        if pos + len > data.len() {
            return Err(format!(
                "TAP block at offset {} claims {} bytes but only {} remain",
                pos - 2,
                len,
                data.len() - pos
            ));
        }

        let block_data = &data[pos..pos + len];
        pos += len;

        let flag = block_data[0];
        let payload = if len >= 2 {
            block_data[1..len - 1].to_vec()
        } else {
            Vec::new()
        };

        blocks.push(TapBlock {
            flag,
            data: payload,
        });
    }

    Ok(blocks)
}

/// Encodes a sequence of raw blocks into TAP file bytes.
///
/// The inverse of [`parse_tap`]: each block is written as a little-endian
/// `u16` length followed by `flag`, the payload, and a trailing XOR checksum
/// (`flag ^ payload[0] ^ … ^ payload[n]`) — exactly the on-tape byte stream the
/// ROM loader produced. `parse_tap(&encode_tap(b)) == b` for any blocks `b`.
#[must_use]
pub fn encode_tap(blocks: &[TapBlock]) -> Vec<u8> {
    let mut out = Vec::new();
    for block in blocks {
        // On-tape length covers the flag, the payload, and the checksum byte.
        let len = block.data.len() + 2;
        out.extend_from_slice(&(len as u16).to_le_bytes());

        let mut checksum = block.flag;
        out.push(block.flag);
        for &byte in &block.data {
            checksum ^= byte;
            out.push(byte);
        }
        out.push(checksum);
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_minimal_header_block() {
        let mut tap = vec![0x13, 0x00];
        tap.push(0x00);
        tap.extend_from_slice(&[0; 17]);
        tap.push(0x00);

        let blocks = parse_tap(&tap).expect("valid TAP should parse");
        assert_eq!(blocks.len(), 1);
        assert!(blocks[0].is_header());
        assert_eq!(blocks[0].flag, 0x00);
        assert_eq!(blocks[0].data.len(), 17);
    }

    #[test]
    fn truncated_block_is_rejected() {
        let tap = [0x05, 0x00, 0xFF, 0x01, 0x02];

        let err = parse_tap(&tap).expect_err("truncated TAP block must fail");
        assert!(err.contains("claims 5 bytes"));
    }

    #[test]
    fn encode_writes_length_prefix_and_xor_checksum() {
        let blocks = vec![TapBlock {
            flag: 0xFF,
            data: vec![0x01, 0x08],
        }];

        // len = flag + 2 payload + checksum = 4; checksum = FF^01^08 = F6.
        assert_eq!(
            encode_tap(&blocks),
            vec![0x04, 0x00, 0xFF, 0x01, 0x08, 0xF6]
        );
    }

    #[test]
    fn encode_then_parse_round_trips() {
        let blocks = vec![
            TapBlock {
                flag: 0x00,
                data: (0..17).collect(),
            },
            TapBlock {
                flag: 0xFF,
                data: vec![0xDE, 0xAD, 0xBE, 0xEF],
            },
        ];

        let parsed = parse_tap(&encode_tap(&blocks)).expect("encoded TAP should parse");
        assert_eq!(parsed, blocks);
    }
}
