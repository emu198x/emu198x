//! C64 TAP file format parser.
//!
//! The C64 TAP format stores raw pulse timing data from the datasette.
//! Each byte encodes a pulse duration; the sequence of short and long
//! pulses represents bits that form bytes and blocks.
//!
//! # TAP header (20 bytes)
//!
//! - Bytes 0-11: Signature "C64-TAPE-RAW"
//! - Byte 12: Version (0 or 1)
//! - Bytes 13-15: Reserved
//! - Bytes 16-19: Data size (little-endian u32)
//!
//! # Pulse encoding
//!
//! Version 0: each byte = duration × 8 CPU cycles (0 = overflow, treated as 256×8).
//! Version 1: byte 0 is a sentinel followed by 3 bytes of exact LE cycle count.
//!
//! # Data encoding (standard C64 tape format)
//!
//! The C64 uses short (~352 cycles) and long (~512 cycles) pulses to encode
//! bits. A short-long pair = 0, a long-short pair = 1. Bytes are transmitted
//! LSB first, each followed by a parity bit (odd parity). Blocks start with
//! a leader of repeated bytes, then a countdown sequence, then data.

#![allow(clippy::cast_possible_truncation)]

/// TAP file header signature.
const TAP_SIGNATURE: &[u8; 12] = b"C64-TAPE-RAW";

/// Pulse duration thresholds (in CPU cycles at 985,248 Hz PAL).
/// These match the standard C64 kernal tape encoding.
const SHORT_THRESHOLD: u32 = 416; // Below this = short pulse
const LONG_THRESHOLD: u32 = 560; // Above this = long pulse
// Between SHORT_THRESHOLD and LONG_THRESHOLD = medium (sync/leader)

/// A decoded C64 tape block.
#[derive(Debug, Clone)]
pub struct C64TapBlock {
    /// File type from the tape header (1=relocatable PRG, 3=non-relocatable, etc.).
    pub file_type: u8,
    /// Load start address.
    pub start_address: u16,
    /// Load end address.
    pub end_address: u16,
    /// Filename (up to 16 characters).
    pub filename: String,
    /// Data payload (the actual program/data bytes).
    pub data: Vec<u8>,
}

/// A parsed C64 TAP file containing decoded blocks.
#[derive(Debug, Clone)]
pub struct C64TapFile {
    /// Decoded tape blocks.
    pub blocks: Vec<C64TapBlock>,
}

/// Classify a pulse duration into short (0), long (1), or medium (sync).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Pulse {
    Short, // 0 bit component
    Long,  // 1 bit component
    Medium, // Leader/sync
}

fn classify_pulse(cycles: u32) -> Pulse {
    if cycles < SHORT_THRESHOLD {
        Pulse::Short
    } else if cycles > LONG_THRESHOLD {
        Pulse::Long
    } else {
        Pulse::Medium
    }
}

/// Extract pulse durations from raw TAP data.
fn extract_pulses(data: &[u8], version: u8) -> Vec<u32> {
    let mut pulses = Vec::new();
    let mut i = 0;
    while i < data.len() {
        if data[i] == 0 && version == 1 {
            // Version 1: 0 sentinel followed by 3-byte LE exact count
            if i + 3 < data.len() {
                let cycles = u32::from(data[i + 1])
                    | (u32::from(data[i + 2]) << 8)
                    | (u32::from(data[i + 3]) << 16);
                pulses.push(cycles);
                i += 4;
            } else {
                break;
            }
        } else {
            // Standard: byte × 8 cycles. Byte 0 in version 0 = 256 × 8.
            let cycles = if data[i] == 0 {
                256 * 8
            } else {
                u32::from(data[i]) * 8
            };
            pulses.push(cycles);
            i += 1;
        }
    }
    pulses
}

/// Decode pulse pairs into a bit stream.
///
/// The C64 tape format uses pulse pairs: short-medium = 0, long-medium = 1
/// (or vice versa depending on the encoding variant). The standard kernal
/// encoding uses:
/// - Short pulse followed by medium pulse = bit 0
/// - Long pulse followed by medium pulse = bit 1
///
/// Returns a vector of bits and the number of pulses consumed.
fn decode_bits(pulses: &[Pulse]) -> Vec<u8> {
    let mut bits = Vec::new();
    let mut i = 0;

    while i + 1 < pulses.len() {
        match (pulses[i], pulses[i + 1]) {
            (Pulse::Short, Pulse::Medium) => {
                bits.push(0);
                i += 2;
            }
            (Pulse::Medium, Pulse::Short) => {
                bits.push(0);
                i += 2;
            }
            (Pulse::Long, Pulse::Medium) => {
                bits.push(1);
                i += 2;
            }
            (Pulse::Medium, Pulse::Long) => {
                bits.push(1);
                i += 2;
            }
            _ => {
                // Skip unrecognized pulse
                i += 1;
            }
        }
    }

    bits
}

/// Decode 8 data bits + 1 parity bit into a byte.
/// Returns (byte, bits_consumed) or None if insufficient bits.
fn decode_byte(bits: &[u8]) -> Option<(u8, usize)> {
    if bits.len() < 9 {
        return None;
    }
    // LSB first, 8 data bits
    let mut byte = 0u8;
    for i in 0..8 {
        if bits[i] != 0 {
            byte |= 1 << i;
        }
    }
    // Skip the parity bit (bit 8) — we don't verify it
    Some((byte, 9))
}

/// Decode a sequence of bytes from a bit stream.
fn decode_bytes(bits: &[u8], count: usize) -> (Vec<u8>, usize) {
    let mut bytes = Vec::with_capacity(count);
    let mut offset = 0;

    for _ in 0..count {
        if let Some((byte, consumed)) = decode_byte(&bits[offset..]) {
            bytes.push(byte);
            offset += consumed;
        } else {
            break;
        }
    }

    (bytes, offset)
}

/// Find the start of a data block by skipping leader and sync.
///
/// The C64 tape leader consists of repeated short pulses (many hundreds),
/// followed by a sync pattern. After sync comes the actual data.
///
/// Returns the index of the first data bit after sync, or None.
fn find_data_start(bits: &[u8], offset: usize) -> Option<usize> {
    // Look for a countdown sequence: $89, $88, $87, ..., $81
    // The countdown uses 9 bits per byte (8 data + 1 parity)
    let mut i = offset;

    // Skip until we find the header marker byte $89
    while i + 9 <= bits.len() {
        if let Some((byte, _)) = decode_byte(&bits[i..]) {
            if byte == 0x89 {
                // Found the start of countdown — skip all 9 countdown bytes
                // ($89 through $81 = 9 bytes × 9 bits = 81 bits)
                let skip = 9 * 9;
                if i + skip <= bits.len() {
                    return Some(i + skip);
                }
                return None;
            }
        }
        i += 1;
    }
    None
}

impl C64TapFile {
    /// Parse a C64 TAP file from raw bytes.
    ///
    /// # Errors
    ///
    /// Returns an error if the header is invalid or the file is truncated.
    pub fn parse(data: &[u8]) -> Result<Self, String> {
        if data.len() < 20 {
            return Err("TAP file too short for header".to_string());
        }

        // Validate signature
        if &data[0..12] != TAP_SIGNATURE {
            return Err("Invalid C64 TAP signature".to_string());
        }

        let version = data[12];
        if version > 1 {
            return Err(format!("Unsupported TAP version: {version}"));
        }

        // Data size (LE u32 at offset 16)
        let data_size = u32::from(data[16])
            | (u32::from(data[17]) << 8)
            | (u32::from(data[18]) << 16)
            | (u32::from(data[19]) << 24);

        let pulse_data_end = 20 + data_size as usize;
        if pulse_data_end > data.len() {
            return Err(format!(
                "TAP data truncated: header says {data_size} bytes, file has {}",
                data.len() - 20
            ));
        }

        let pulse_data = &data[20..pulse_data_end];

        // Extract pulse durations
        let raw_pulses = extract_pulses(pulse_data, version);

        // Classify pulses
        let pulses: Vec<Pulse> = raw_pulses.iter().map(|&c| classify_pulse(c)).collect();

        // Decode pulse pairs to bits
        let bits = decode_bits(&pulses);

        // Find and decode blocks
        let blocks = Self::extract_blocks(&bits);

        Ok(Self { blocks })
    }

    /// Extract blocks from a decoded bit stream.
    ///
    /// The C64 kernal records each block twice on tape. We extract both
    /// copies but only keep unique blocks (by matching file type and start
    /// address, taking the first copy).
    fn extract_blocks(bits: &[u8]) -> Vec<C64TapBlock> {
        let mut blocks = Vec::new();
        let mut offset = 0;

        // Scan for blocks
        loop {
            // Find next data start (after leader/sync/countdown)
            let Some(data_start) = find_data_start(bits, offset) else {
                break;
            };

            // Try to decode a tape header (192 bytes)
            // First byte after countdown is the block type marker
            let remaining = &bits[data_start..];
            let (header_bytes, consumed) = decode_bytes(remaining, 192);

            if header_bytes.len() < 192 {
                offset = data_start + consumed.max(1);
                continue;
            }

            // Parse the 192-byte header:
            // Byte 0: file type (1=BASIC, 3=ML, 4=SEQ, 5=end-of-tape marker)
            let file_type = header_bytes[0];

            // Bytes 1-2: start address (lo/hi)
            let start_address =
                u16::from(header_bytes[1]) | (u16::from(header_bytes[2]) << 8);

            // Bytes 3-4: end address (lo/hi)
            let end_address =
                u16::from(header_bytes[3]) | (u16::from(header_bytes[4]) << 8);

            // Bytes 5-20: filename (16 characters, padded with spaces/$A0)
            let name_bytes = &header_bytes[5..21];
            let filename = name_bytes
                .iter()
                .map(|&b| if b == 0xA0 || b == 0 { ' ' } else { b as char })
                .collect::<String>()
                .trim()
                .to_string();

            // Move past the header block (192 bytes × 9 bits + gap)
            offset = data_start + consumed;

            // Skip the repeated copy of the header
            if let Some(repeat_start) = find_data_start(bits, offset) {
                // Skip the repeat header (192 × 9 bits)
                let skip_bits = 192 * 9;
                offset = (repeat_start + skip_bits).min(bits.len());
            }

            // End-of-tape marker — stop scanning
            if file_type == 5 {
                continue;
            }

            // Now find the data block
            let data_len = if end_address > start_address {
                (end_address - start_address) as usize
            } else {
                0
            };

            if data_len == 0 {
                continue;
            }

            // Find data block start
            let Some(payload_start) = find_data_start(bits, offset) else {
                break;
            };

            // First byte of data block is another type marker; skip it
            let remaining = &bits[payload_start..];

            // Decode type marker + data bytes
            let (mut payload_bytes, payload_consumed) = decode_bytes(remaining, data_len + 1);
            offset = payload_start + payload_consumed;

            // Skip the repeated data block
            if let Some(repeat_start) = find_data_start(bits, offset) {
                let skip_bits = (data_len + 1) * 9;
                offset = (repeat_start + skip_bits).min(bits.len());
            }

            // Remove the type marker byte from the front
            if !payload_bytes.is_empty() {
                payload_bytes.remove(0);
            }

            blocks.push(C64TapBlock {
                file_type,
                start_address,
                end_address,
                filename,
                data: payload_bytes,
            });
        }

        blocks
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_header_v0() {
        let mut data = vec![0u8; 20];
        data[0..12].copy_from_slice(TAP_SIGNATURE);
        data[12] = 0; // Version 0
        // Data size = 0 (empty pulse stream)
        let tap = C64TapFile::parse(&data).expect("empty v0 TAP should parse");
        assert!(tap.blocks.is_empty());
    }

    #[test]
    fn parse_header_v1() {
        let mut data = vec![0u8; 20];
        data[0..12].copy_from_slice(TAP_SIGNATURE);
        data[12] = 1; // Version 1
        let tap = C64TapFile::parse(&data).expect("empty v1 TAP should parse");
        assert!(tap.blocks.is_empty());
    }

    #[test]
    fn parse_bad_signature() {
        let data = vec![0u8; 20];
        assert!(C64TapFile::parse(&data).is_err());
    }

    #[test]
    fn parse_too_short() {
        assert!(C64TapFile::parse(&[0; 10]).is_err());
    }

    #[test]
    fn parse_unsupported_version() {
        let mut data = vec![0u8; 20];
        data[0..12].copy_from_slice(TAP_SIGNATURE);
        data[12] = 2; // Unsupported
        assert!(C64TapFile::parse(&data).is_err());
    }

    #[test]
    fn pulse_classification() {
        // Short: < 416 cycles
        assert_eq!(classify_pulse(300), Pulse::Short);
        assert_eq!(classify_pulse(415), Pulse::Short);
        // Medium: 416-560 cycles
        assert_eq!(classify_pulse(416), Pulse::Medium);
        assert_eq!(classify_pulse(500), Pulse::Medium);
        assert_eq!(classify_pulse(560), Pulse::Medium);
        // Long: > 560 cycles
        assert_eq!(classify_pulse(561), Pulse::Long);
        assert_eq!(classify_pulse(1000), Pulse::Long);
    }

    #[test]
    fn extract_pulses_v0() {
        // Version 0: each byte × 8 cycles
        let data = vec![44, 64, 0]; // 352, 512, 2048 (256×8 for 0)
        let pulses = extract_pulses(&data, 0);
        assert_eq!(pulses, vec![352, 512, 2048]);
    }

    #[test]
    fn extract_pulses_v1_sentinel() {
        // Version 1: byte 0 followed by 3-byte LE exact count
        let data = vec![44, 0, 0x60, 0x01, 0x00, 64]; // 352, 352 (exact), 512
        let pulses = extract_pulses(&data, 1);
        assert_eq!(pulses, vec![352, 0x160, 512]); // 0x160 = 352
    }

    #[test]
    fn decode_byte_lsb_first() {
        // Byte 0x55 = 01010101 binary, LSB first: 1,0,1,0,1,0,1,0
        // Parity bit (odd parity of 4 ones) = 0
        let bits = [1, 0, 1, 0, 1, 0, 1, 0, 0];
        let (byte, consumed) = decode_byte(&bits).expect("should decode");
        assert_eq!(byte, 0x55);
        assert_eq!(consumed, 9);
    }

    #[test]
    fn decode_byte_all_ones() {
        // 0xFF = 11111111, LSB first: 1,1,1,1,1,1,1,1
        // Odd parity of 8 ones = 0
        let bits = [1, 1, 1, 1, 1, 1, 1, 1, 0];
        let (byte, _) = decode_byte(&bits).expect("should decode");
        assert_eq!(byte, 0xFF);
    }

    #[test]
    fn decode_byte_insufficient_bits() {
        let bits = [0, 1, 0, 1]; // Only 4 bits
        assert!(decode_byte(&bits).is_none());
    }

    #[test]
    fn truncated_data() {
        let mut data = vec![0u8; 20];
        data[0..12].copy_from_slice(TAP_SIGNATURE);
        data[12] = 0;
        // Say data size is 100 but file is only 20 bytes
        data[16] = 100;
        assert!(C64TapFile::parse(&data).is_err());
    }
}
