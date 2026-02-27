//! TZX file format parser.
//!
//! TZX encodes tape signals as T-state-accurate pulse sequences. Unlike TAP
//! (which relies on a ROM trap for instant loading), TZX drives the EAR bit
//! in real time — supporting turbo loaders, custom protection, and any
//! non-ROM loading routine.
//!
//! # Format
//!
//! A TZX file starts with a 10-byte header (`"ZXTape!" + 0x1A + major + minor`)
//! followed by a sequence of blocks. Each block starts with an ID byte that
//! determines its structure.
//!
//! Reference: <https://worldofspectrum.net/TZXformat.html>

#![allow(clippy::cast_possible_truncation)]

/// A parsed TZX file.
#[derive(Debug, Clone)]
pub struct TzxFile {
    pub major: u8,
    pub minor: u8,
    pub blocks: Vec<TzxBlock>,
}

/// A single TZX block.
#[derive(Debug, Clone)]
pub enum TzxBlock {
    /// Block $10: Standard speed data (ROM timing).
    StandardSpeed {
        pause_ms: u16,
        data: Vec<u8>,
    },
    /// Block $11: Turbo speed data (custom timing).
    TurboSpeed {
        pilot_pulse: u16,
        sync1: u16,
        sync2: u16,
        zero_pulse: u16,
        one_pulse: u16,
        pilot_count: u16,
        used_bits: u8,
        pause_ms: u16,
        data: Vec<u8>,
    },
    /// Block $12: Pure tone (repeated single pulse).
    PureTone {
        pulse_len: u16,
        count: u16,
    },
    /// Block $13: Pulse sequence (arbitrary pulse lengths).
    PulseSequence {
        pulses: Vec<u16>,
    },
    /// Block $14: Pure data (no pilot or sync, just data bits).
    PureData {
        zero_pulse: u16,
        one_pulse: u16,
        used_bits: u8,
        pause_ms: u16,
        data: Vec<u8>,
    },
    /// Block $20: Pause / stop the tape.
    Pause {
        duration_ms: u16,
    },
    /// Block $21: Group start.
    GroupStart {
        name: String,
    },
    /// Block $22: Group end.
    GroupEnd,
    /// Block $24: Loop start.
    LoopStart {
        repetitions: u16,
    },
    /// Block $25: Loop end.
    LoopEnd,
    /// Block $2A: Stop the tape if in 48K mode.
    StopIf48K,
    /// Block $2B: Set signal level.
    SetSignalLevel {
        level: bool,
    },
    /// Block $30: Text description.
    TextDescription {
        text: String,
    },
    /// Block $32: Archive info.
    ArchiveInfo {
        entries: Vec<(u8, String)>,
    },
    /// Unknown or unsupported block (skipped gracefully).
    Unknown {
        block_id: u8,
    },
}

/// TZX header magic: "ZXTape!" + 0x1A.
const MAGIC: &[u8; 8] = b"ZXTape!\x1A";

impl TzxFile {
    /// Parse a TZX file from raw bytes.
    ///
    /// # Errors
    ///
    /// Returns an error if the header is invalid or a block is malformed.
    pub fn parse(data: &[u8]) -> Result<Self, String> {
        if data.len() < 10 {
            return Err("TZX file too short for header (need 10 bytes)".to_string());
        }

        if &data[0..8] != MAGIC {
            return Err("Invalid TZX magic (expected \"ZXTape!\" + 0x1A)".to_string());
        }

        let major = data[8];
        let minor = data[9];
        let mut blocks = Vec::new();
        let mut pos = 10;

        while pos < data.len() {
            let block_id = data[pos];
            pos += 1;

            let block = match block_id {
                0x10 => parse_standard_speed(data, &mut pos)?,
                0x11 => parse_turbo_speed(data, &mut pos)?,
                0x12 => parse_pure_tone(data, &mut pos)?,
                0x13 => parse_pulse_sequence(data, &mut pos)?,
                0x14 => parse_pure_data(data, &mut pos)?,
                0x20 => parse_pause(data, &mut pos)?,
                0x21 => parse_group_start(data, &mut pos)?,
                0x22 => TzxBlock::GroupEnd,
                0x24 => parse_loop_start(data, &mut pos)?,
                0x25 => TzxBlock::LoopEnd,
                0x2A => parse_stop_if_48k(data, &mut pos)?,
                0x2B => parse_set_signal_level(data, &mut pos)?,
                0x30 => parse_text_description(data, &mut pos)?,
                0x32 => parse_archive_info(data, &mut pos)?,
                _ => skip_unknown_block(block_id, data, &mut pos)?,
            };

            blocks.push(block);
        }

        Ok(Self {
            major,
            minor,
            blocks,
        })
    }
}

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

fn need(data: &[u8], pos: usize, n: usize, ctx: &str) -> Result<(), String> {
    if pos + n > data.len() {
        Err(format!(
            "Truncated TZX {ctx} at offset {pos}: need {n} bytes, {} remain",
            data.len() - pos
        ))
    } else {
        Ok(())
    }
}

fn read_u16_le(data: &[u8], pos: usize) -> u16 {
    u16::from(data[pos]) | (u16::from(data[pos + 1]) << 8)
}

fn read_u24_le(data: &[u8], pos: usize) -> u32 {
    u32::from(data[pos]) | (u32::from(data[pos + 1]) << 8) | (u32::from(data[pos + 2]) << 16)
}

fn read_u32_le(data: &[u8], pos: usize) -> u32 {
    u32::from(data[pos])
        | (u32::from(data[pos + 1]) << 8)
        | (u32::from(data[pos + 2]) << 16)
        | (u32::from(data[pos + 3]) << 24)
}

// ---------------------------------------------------------------------------
// Block parsers
// ---------------------------------------------------------------------------

/// Block $10: Standard speed data block.
fn parse_standard_speed(data: &[u8], pos: &mut usize) -> Result<TzxBlock, String> {
    need(data, *pos, 4, "Standard Speed header")?;
    let pause_ms = read_u16_le(data, *pos);
    let data_len = read_u16_le(data, *pos + 2) as usize;
    *pos += 4;

    need(data, *pos, data_len, "Standard Speed data")?;
    let block_data = data[*pos..*pos + data_len].to_vec();
    *pos += data_len;

    Ok(TzxBlock::StandardSpeed {
        pause_ms,
        data: block_data,
    })
}

/// Block $11: Turbo speed data block.
fn parse_turbo_speed(data: &[u8], pos: &mut usize) -> Result<TzxBlock, String> {
    need(data, *pos, 18, "Turbo Speed header")?;
    let pilot_pulse = read_u16_le(data, *pos);
    let sync1 = read_u16_le(data, *pos + 2);
    let sync2 = read_u16_le(data, *pos + 4);
    let zero_pulse = read_u16_le(data, *pos + 6);
    let one_pulse = read_u16_le(data, *pos + 8);
    let pilot_count = read_u16_le(data, *pos + 10);
    let used_bits = data[*pos + 12];
    let pause_ms = read_u16_le(data, *pos + 13);
    let data_len = read_u24_le(data, *pos + 15) as usize;
    *pos += 18;

    need(data, *pos, data_len, "Turbo Speed data")?;
    let block_data = data[*pos..*pos + data_len].to_vec();
    *pos += data_len;

    Ok(TzxBlock::TurboSpeed {
        pilot_pulse,
        sync1,
        sync2,
        zero_pulse,
        one_pulse,
        pilot_count,
        used_bits,
        pause_ms,
        data: block_data,
    })
}

/// Block $12: Pure tone.
fn parse_pure_tone(data: &[u8], pos: &mut usize) -> Result<TzxBlock, String> {
    need(data, *pos, 4, "Pure Tone")?;
    let pulse_len = read_u16_le(data, *pos);
    let count = read_u16_le(data, *pos + 2);
    *pos += 4;
    Ok(TzxBlock::PureTone { pulse_len, count })
}

/// Block $13: Pulse sequence.
fn parse_pulse_sequence(data: &[u8], pos: &mut usize) -> Result<TzxBlock, String> {
    need(data, *pos, 1, "Pulse Sequence count")?;
    let count = data[*pos] as usize;
    *pos += 1;

    need(data, *pos, count * 2, "Pulse Sequence data")?;
    let mut pulses = Vec::with_capacity(count);
    for i in 0..count {
        pulses.push(read_u16_le(data, *pos + i * 2));
    }
    *pos += count * 2;

    Ok(TzxBlock::PulseSequence { pulses })
}

/// Block $14: Pure data block.
fn parse_pure_data(data: &[u8], pos: &mut usize) -> Result<TzxBlock, String> {
    need(data, *pos, 10, "Pure Data header")?;
    let zero_pulse = read_u16_le(data, *pos);
    let one_pulse = read_u16_le(data, *pos + 2);
    let used_bits = data[*pos + 4];
    let pause_ms = read_u16_le(data, *pos + 5);
    let data_len = read_u24_le(data, *pos + 7) as usize;
    *pos += 10;

    need(data, *pos, data_len, "Pure Data data")?;
    let block_data = data[*pos..*pos + data_len].to_vec();
    *pos += data_len;

    Ok(TzxBlock::PureData {
        zero_pulse,
        one_pulse,
        used_bits,
        pause_ms,
        data: block_data,
    })
}

/// Block $20: Pause / stop the tape.
fn parse_pause(data: &[u8], pos: &mut usize) -> Result<TzxBlock, String> {
    need(data, *pos, 2, "Pause")?;
    let duration_ms = read_u16_le(data, *pos);
    *pos += 2;
    Ok(TzxBlock::Pause { duration_ms })
}

/// Block $21: Group start.
fn parse_group_start(data: &[u8], pos: &mut usize) -> Result<TzxBlock, String> {
    need(data, *pos, 1, "Group Start length")?;
    let len = data[*pos] as usize;
    *pos += 1;

    need(data, *pos, len, "Group Start name")?;
    let name = String::from_utf8_lossy(&data[*pos..*pos + len]).to_string();
    *pos += len;

    Ok(TzxBlock::GroupStart { name })
}

/// Block $24: Loop start.
fn parse_loop_start(data: &[u8], pos: &mut usize) -> Result<TzxBlock, String> {
    need(data, *pos, 2, "Loop Start")?;
    let repetitions = read_u16_le(data, *pos);
    *pos += 2;
    Ok(TzxBlock::LoopStart { repetitions })
}

/// Block $2A: Stop the tape if in 48K mode.
fn parse_stop_if_48k(data: &[u8], pos: &mut usize) -> Result<TzxBlock, String> {
    need(data, *pos, 4, "Stop If 48K")?;
    // 4-byte block length (always 0 for this block type)
    *pos += 4;
    Ok(TzxBlock::StopIf48K)
}

/// Block $2B: Set signal level.
fn parse_set_signal_level(data: &[u8], pos: &mut usize) -> Result<TzxBlock, String> {
    need(data, *pos, 5, "Set Signal Level")?;
    // 4-byte block length (always 1) + 1-byte level
    let level = data[*pos + 4] != 0;
    *pos += 5;
    Ok(TzxBlock::SetSignalLevel { level })
}

/// Block $30: Text description.
fn parse_text_description(data: &[u8], pos: &mut usize) -> Result<TzxBlock, String> {
    need(data, *pos, 1, "Text Description length")?;
    let len = data[*pos] as usize;
    *pos += 1;

    need(data, *pos, len, "Text Description text")?;
    let text = String::from_utf8_lossy(&data[*pos..*pos + len]).to_string();
    *pos += len;

    Ok(TzxBlock::TextDescription { text })
}

/// Block $32: Archive info.
fn parse_archive_info(data: &[u8], pos: &mut usize) -> Result<TzxBlock, String> {
    need(data, *pos, 2, "Archive Info header")?;
    let block_len = read_u16_le(data, *pos) as usize;
    *pos += 2;

    need(data, *pos, block_len, "Archive Info data")?;
    let block_end = *pos + block_len;

    if block_len < 1 {
        return Err("Archive Info block too short".to_string());
    }

    let num_entries = data[*pos] as usize;
    *pos += 1;

    let mut entries = Vec::with_capacity(num_entries);
    for _ in 0..num_entries {
        if *pos + 2 > block_end {
            break;
        }
        let entry_id = data[*pos];
        let entry_len = data[*pos + 1] as usize;
        *pos += 2;

        let text_end = (*pos + entry_len).min(block_end);
        let text = String::from_utf8_lossy(&data[*pos..text_end]).to_string();
        *pos = text_end;

        entries.push((entry_id, text));
    }

    // Skip any remaining bytes in the block
    *pos = block_end;

    Ok(TzxBlock::ArchiveInfo { entries })
}

/// Skip an unknown block using known length schemes, or a 4-byte length prefix.
fn skip_unknown_block(block_id: u8, data: &[u8], pos: &mut usize) -> Result<TzxBlock, String> {
    // Blocks with known length layout
    let skip_len = match block_id {
        // $15: Direct recording — 8-byte header + 3-byte data length
        0x15 => {
            need(data, *pos, 8, "Direct Recording header")?;
            let data_len = read_u24_le(data, *pos + 5) as usize;
            8 + data_len
        }
        // $18: CSW recording — 4-byte block length
        // $19: Generalized data — 4-byte block length
        0x18 | 0x19 => {
            need(data, *pos, 4, "block length")?;
            read_u32_le(data, *pos) as usize + 4
        }
        // $23: Call sequence — 2-byte count * 2 + 2
        0x23 => {
            need(data, *pos, 2, "Call Sequence count")?;
            let count = read_u16_le(data, *pos) as usize;
            2 + count * 2
        }
        // $26: Return from sequence — no data
        0x26 => 0,
        // $27: Select block — 2-byte length prefix
        0x27 => {
            need(data, *pos, 2, "Select Block length")?;
            read_u16_le(data, *pos) as usize + 2
        }
        // $28: Jump to block — 2-byte length prefix
        0x28 => {
            need(data, *pos, 2, "Jump To length")?;
            read_u16_le(data, *pos) as usize + 2
        }
        // $33: Hardware type — 1-byte count * 3 + 1
        0x33 => {
            need(data, *pos, 1, "Hardware Type count")?;
            let count = data[*pos] as usize;
            1 + count * 3
        }
        // $35: Custom info — 16-byte ID + 4-byte length
        0x35 => {
            need(data, *pos, 20, "Custom Info header")?;
            let len = read_u32_le(data, *pos + 16) as usize;
            20 + len
        }
        // $5A: "Glue" block (merge point) — 9 bytes
        0x5A => 9,
        // For truly unknown blocks, try 4-byte length prefix as a last resort
        _ => {
            if *pos + 4 <= data.len() {
                let len = read_u32_le(data, *pos) as usize;
                4 + len
            } else {
                return Err(format!(
                    "Unknown TZX block ${block_id:02X} at offset {} with no way to determine length",
                    *pos - 1
                ));
            }
        }
    };

    need(data, *pos, skip_len, &format!("Unknown block ${block_id:02X}"))?;
    *pos += skip_len;

    Ok(TzxBlock::Unknown { block_id })
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    /// Build a minimal TZX file from a header + raw block bytes.
    fn tzx_header() -> Vec<u8> {
        let mut h = MAGIC.to_vec();
        h.push(1); // major
        h.push(20); // minor
        h
    }

    #[test]
    fn parse_valid_header_empty() {
        let data = tzx_header();
        let tzx = TzxFile::parse(&data).expect("valid empty TZX");
        assert_eq!(tzx.major, 1);
        assert_eq!(tzx.minor, 20);
        assert!(tzx.blocks.is_empty());
    }

    #[test]
    fn parse_too_short() {
        assert!(TzxFile::parse(&[]).is_err());
        assert!(TzxFile::parse(&[0; 9]).is_err());
    }

    #[test]
    fn parse_bad_magic() {
        let mut data = tzx_header();
        data[0] = b'X';
        assert!(TzxFile::parse(&data).is_err());
    }

    #[test]
    fn parse_standard_speed_block() {
        let mut data = tzx_header();
        data.push(0x10); // block ID
        data.extend_from_slice(&1000u16.to_le_bytes()); // pause_ms
        let payload = [0x00, 0x01, 0x02, 0x03]; // flag + 3 bytes
        data.extend_from_slice(&(payload.len() as u16).to_le_bytes());
        data.extend_from_slice(&payload);

        let tzx = TzxFile::parse(&data).expect("standard speed block");
        assert_eq!(tzx.blocks.len(), 1);
        match &tzx.blocks[0] {
            TzxBlock::StandardSpeed {
                pause_ms,
                data: block_data,
            } => {
                assert_eq!(*pause_ms, 1000);
                assert_eq!(block_data, &[0x00, 0x01, 0x02, 0x03]);
            }
            _ => panic!("Expected StandardSpeed"),
        }
    }

    #[test]
    fn parse_turbo_speed_block() {
        let mut data = tzx_header();
        data.push(0x11);
        data.extend_from_slice(&2168u16.to_le_bytes()); // pilot_pulse
        data.extend_from_slice(&667u16.to_le_bytes()); // sync1
        data.extend_from_slice(&735u16.to_le_bytes()); // sync2
        data.extend_from_slice(&855u16.to_le_bytes()); // zero
        data.extend_from_slice(&1710u16.to_le_bytes()); // one
        data.extend_from_slice(&3223u16.to_le_bytes()); // pilot_count
        data.push(8); // used_bits
        data.extend_from_slice(&1000u16.to_le_bytes()); // pause_ms
        let payload = [0xFF, 0xAA];
        // 3-byte LE data length
        data.push(payload.len() as u8);
        data.push(0);
        data.push(0);
        data.extend_from_slice(&payload);

        let tzx = TzxFile::parse(&data).expect("turbo speed block");
        assert_eq!(tzx.blocks.len(), 1);
        match &tzx.blocks[0] {
            TzxBlock::TurboSpeed {
                pilot_pulse,
                sync1,
                sync2,
                zero_pulse,
                one_pulse,
                pilot_count,
                used_bits,
                pause_ms,
                data: block_data,
            } => {
                assert_eq!(*pilot_pulse, 2168);
                assert_eq!(*sync1, 667);
                assert_eq!(*sync2, 735);
                assert_eq!(*zero_pulse, 855);
                assert_eq!(*one_pulse, 1710);
                assert_eq!(*pilot_count, 3223);
                assert_eq!(*used_bits, 8);
                assert_eq!(*pause_ms, 1000);
                assert_eq!(block_data, &[0xFF, 0xAA]);
            }
            _ => panic!("Expected TurboSpeed"),
        }
    }

    #[test]
    fn parse_pure_tone_block() {
        let mut data = tzx_header();
        data.push(0x12);
        data.extend_from_slice(&2168u16.to_le_bytes());
        data.extend_from_slice(&8063u16.to_le_bytes());

        let tzx = TzxFile::parse(&data).expect("pure tone");
        match &tzx.blocks[0] {
            TzxBlock::PureTone { pulse_len, count } => {
                assert_eq!(*pulse_len, 2168);
                assert_eq!(*count, 8063);
            }
            _ => panic!("Expected PureTone"),
        }
    }

    #[test]
    fn parse_pulse_sequence_block() {
        let mut data = tzx_header();
        data.push(0x13);
        data.push(3); // 3 pulses
        data.extend_from_slice(&100u16.to_le_bytes());
        data.extend_from_slice(&200u16.to_le_bytes());
        data.extend_from_slice(&300u16.to_le_bytes());

        let tzx = TzxFile::parse(&data).expect("pulse sequence");
        match &tzx.blocks[0] {
            TzxBlock::PulseSequence { pulses } => {
                assert_eq!(pulses, &[100, 200, 300]);
            }
            _ => panic!("Expected PulseSequence"),
        }
    }

    #[test]
    fn parse_pure_data_block() {
        let mut data = tzx_header();
        data.push(0x14);
        data.extend_from_slice(&855u16.to_le_bytes()); // zero
        data.extend_from_slice(&1710u16.to_le_bytes()); // one
        data.push(6); // used_bits
        data.extend_from_slice(&500u16.to_le_bytes()); // pause_ms
        let payload = [0xAB];
        data.push(payload.len() as u8);
        data.push(0);
        data.push(0);
        data.extend_from_slice(&payload);

        let tzx = TzxFile::parse(&data).expect("pure data");
        match &tzx.blocks[0] {
            TzxBlock::PureData {
                zero_pulse,
                one_pulse,
                used_bits,
                pause_ms,
                data: block_data,
            } => {
                assert_eq!(*zero_pulse, 855);
                assert_eq!(*one_pulse, 1710);
                assert_eq!(*used_bits, 6);
                assert_eq!(*pause_ms, 500);
                assert_eq!(block_data, &[0xAB]);
            }
            _ => panic!("Expected PureData"),
        }
    }

    #[test]
    fn parse_pause_block() {
        let mut data = tzx_header();
        data.push(0x20);
        data.extend_from_slice(&2000u16.to_le_bytes());

        let tzx = TzxFile::parse(&data).expect("pause");
        match &tzx.blocks[0] {
            TzxBlock::Pause { duration_ms } => assert_eq!(*duration_ms, 2000),
            _ => panic!("Expected Pause"),
        }
    }

    #[test]
    fn parse_group_start_end() {
        let mut data = tzx_header();
        // Group start
        data.push(0x21);
        let name = b"Level 1";
        data.push(name.len() as u8);
        data.extend_from_slice(name);
        // Group end
        data.push(0x22);

        let tzx = TzxFile::parse(&data).expect("group");
        assert_eq!(tzx.blocks.len(), 2);
        match &tzx.blocks[0] {
            TzxBlock::GroupStart { name } => assert_eq!(name, "Level 1"),
            _ => panic!("Expected GroupStart"),
        }
        assert!(matches!(tzx.blocks[1], TzxBlock::GroupEnd));
    }

    #[test]
    fn parse_loop_start_end() {
        let mut data = tzx_header();
        data.push(0x24);
        data.extend_from_slice(&5u16.to_le_bytes());
        data.push(0x25);

        let tzx = TzxFile::parse(&data).expect("loop");
        assert_eq!(tzx.blocks.len(), 2);
        match &tzx.blocks[0] {
            TzxBlock::LoopStart { repetitions } => assert_eq!(*repetitions, 5),
            _ => panic!("Expected LoopStart"),
        }
        assert!(matches!(tzx.blocks[1], TzxBlock::LoopEnd));
    }

    #[test]
    fn parse_stop_if_48k() {
        let mut data = tzx_header();
        data.push(0x2A);
        data.extend_from_slice(&0u32.to_le_bytes()); // block length = 0

        let tzx = TzxFile::parse(&data).expect("stop if 48k");
        assert!(matches!(tzx.blocks[0], TzxBlock::StopIf48K));
    }

    #[test]
    fn parse_set_signal_level() {
        let mut data = tzx_header();
        data.push(0x2B);
        data.extend_from_slice(&1u32.to_le_bytes()); // block length = 1
        data.push(1); // level = high

        let tzx = TzxFile::parse(&data).expect("set signal level");
        match &tzx.blocks[0] {
            TzxBlock::SetSignalLevel { level } => assert!(*level),
            _ => panic!("Expected SetSignalLevel"),
        }
    }

    #[test]
    fn parse_text_description() {
        let mut data = tzx_header();
        data.push(0x30);
        let text = b"Hello World";
        data.push(text.len() as u8);
        data.extend_from_slice(text);

        let tzx = TzxFile::parse(&data).expect("text description");
        match &tzx.blocks[0] {
            TzxBlock::TextDescription { text } => assert_eq!(text, "Hello World"),
            _ => panic!("Expected TextDescription"),
        }
    }

    #[test]
    fn parse_archive_info() {
        let mut data = tzx_header();
        data.push(0x32);
        // Block length: 1 (count) + 2+5 (entry 1) + 2+3 (entry 2) = 13
        data.extend_from_slice(&13u16.to_le_bytes());
        data.push(2); // 2 entries
        // Entry 1: id=0x00 (Title), "Hello"
        data.push(0x00);
        data.push(5);
        data.extend_from_slice(b"Hello");
        // Entry 2: id=0x02 (Author), "Bob"
        data.push(0x02);
        data.push(3);
        data.extend_from_slice(b"Bob");

        let tzx = TzxFile::parse(&data).expect("archive info");
        match &tzx.blocks[0] {
            TzxBlock::ArchiveInfo { entries } => {
                assert_eq!(entries.len(), 2);
                assert_eq!(entries[0], (0x00, "Hello".to_string()));
                assert_eq!(entries[1], (0x02, "Bob".to_string()));
            }
            _ => panic!("Expected ArchiveInfo"),
        }
    }

    #[test]
    fn unknown_block_skipped() {
        let mut data = tzx_header();
        // Use block ID $5A (Glue) — known to be 9 bytes
        data.push(0x5A);
        data.extend_from_slice(&[0u8; 9]);

        let tzx = TzxFile::parse(&data).expect("unknown block skipped");
        assert_eq!(tzx.blocks.len(), 1);
        match &tzx.blocks[0] {
            TzxBlock::Unknown { block_id } => assert_eq!(*block_id, 0x5A),
            _ => panic!("Expected Unknown"),
        }
    }

    #[test]
    fn multiple_blocks_in_sequence() {
        let mut data = tzx_header();

        // Block $30: Text
        data.push(0x30);
        let text = b"Test";
        data.push(text.len() as u8);
        data.extend_from_slice(text);

        // Block $12: Pure tone
        data.push(0x12);
        data.extend_from_slice(&1000u16.to_le_bytes());
        data.extend_from_slice(&100u16.to_le_bytes());

        // Block $20: Pause
        data.push(0x20);
        data.extend_from_slice(&500u16.to_le_bytes());

        let tzx = TzxFile::parse(&data).expect("multiple blocks");
        assert_eq!(tzx.blocks.len(), 3);
        assert!(matches!(tzx.blocks[0], TzxBlock::TextDescription { .. }));
        assert!(matches!(tzx.blocks[1], TzxBlock::PureTone { .. }));
        assert!(matches!(tzx.blocks[2], TzxBlock::Pause { .. }));
    }

    #[test]
    fn truncated_block_errors() {
        let mut data = tzx_header();
        data.push(0x10); // Standard speed, but no data following
        assert!(TzxFile::parse(&data).is_err());
    }
}
