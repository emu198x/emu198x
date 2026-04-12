//! TAP tape format parser for the Sinclair ZX Spectrum.
//!
//! Source references:
//! - `wiki/concepts/tape-formats.md`
//! - Adapted from `/Users/stevehill/Projects/Emu198x-Older/crates/format-sinclair-zx-spectrum-tap/src/lib.rs`
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
}
