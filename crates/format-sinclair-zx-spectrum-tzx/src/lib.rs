//! TZX tape format parser for the Sinclair ZX Spectrum.
//!
//! Source references:
//! - `wiki/concepts/tape-formats.md`
//! - Adapted from `/Users/stevehill/Projects/Emu198x-Older/crates/format-sinclair-zx-spectrum-tzx/src/lib.rs`
//!
//! The parser converts TZX blocks into a flat pulse stream in CPU T-states.
//! Tape playback later consumes those pulse lengths directly.

const PILOT_PULSE: u32 = 2_168;
const SYNC1_PULSE: u32 = 667;
const SYNC2_PULSE: u32 = 735;
const ZERO_PULSE: u32 = 855;
const ONE_PULSE: u32 = 1_710;
const PILOT_COUNT_HEADER: u32 = 8_063;
const PILOT_COUNT_DATA: u32 = 3_223;
const TSTATES_PER_MS: u32 = 3_500;

/// Parses a TZX file and returns a flat pulse sequence in T-states.
///
/// # Errors
///
/// Returns an error if the file header is invalid, the version is unsupported,
/// a block overruns the supplied bytes, or the file contains an unknown block.
pub fn tzx_to_pulses(data: &[u8]) -> Result<Vec<u32>, String> {
    if data.len() < 10 {
        return Err("TZX file too short".into());
    }
    if &data[0..7] != b"ZXTape!" || data[7] != 0x1A {
        return Err("Not a valid TZX file (bad header)".into());
    }

    let major = data[8];
    if major > 1 {
        return Err(format!("Unsupported TZX version {}.{}", major, data[9]));
    }

    let mut pulses = Vec::new();
    let mut pos = 10usize;
    let mut loop_start_pos: Option<usize> = None;
    let mut loop_start_pulse_len = 0usize;
    let mut loop_remaining = 0u16;

    while pos < data.len() {
        let block_id = data[pos];
        pos += 1;

        match block_id {
            0x10 => pos = parse_standard_speed(data, pos, &mut pulses)?,
            0x11 => pos = parse_turbo_speed(data, pos, &mut pulses)?,
            0x12 => pos = parse_pure_tone(data, pos, &mut pulses)?,
            0x13 => pos = parse_pulse_sequence(data, pos, &mut pulses)?,
            0x14 => pos = parse_pure_data(data, pos, &mut pulses)?,
            0x15 => pos = parse_direct_recording(data, pos, &mut pulses)?,
            0x20 => pos = parse_pause(data, pos, &mut pulses)?,
            0x24 => {
                check_len(data, pos, 2, "Loop Start")?;
                loop_remaining = read_u16(data, pos);
                loop_start_pos = Some(pos + 2);
                loop_start_pulse_len = pulses.len();
                pos += 2;
            }
            0x25 => {
                if let Some(start_pos) = loop_start_pos {
                    if loop_remaining > 1 {
                        let body = pulses[loop_start_pulse_len..].to_vec();
                        for _ in 1..loop_remaining {
                            pulses.extend_from_slice(&body);
                        }
                    }
                    loop_start_pos = None;
                    let _ = start_pos;
                }
            }
            0x21 => {
                check_len(data, pos, 1, "Group Start")?;
                pos += 1 + usize::from(data[pos]);
            }
            0x22 => {}
            0x23 => {
                check_len(data, pos, 2, "Jump To Block")?;
                pos += 2;
            }
            0x26 => {
                check_len(data, pos, 2, "Call Sequence")?;
                let count = usize::from(read_u16(data, pos));
                pos += 2 + count * 2;
            }
            0x27 => {}
            0x28 => {
                check_len(data, pos, 2, "Select Block")?;
                let len = usize::from(read_u16(data, pos));
                pos += 2 + len;
            }
            0x2A => {
                check_len(data, pos, 4, "Stop 48K")?;
                let len = read_u32(data, pos) as usize;
                pos += 4 + len;
            }
            0x2B => {
                check_len(data, pos, 4, "Set Signal Level")?;
                let len = read_u32(data, pos) as usize;
                pos += 4 + len;
            }
            0x30 => {
                check_len(data, pos, 1, "Text Description")?;
                pos += 1 + usize::from(data[pos]);
            }
            0x31 => {
                check_len(data, pos, 2, "Message Block")?;
                pos += 2 + usize::from(data[pos + 1]);
            }
            0x32 => {
                check_len(data, pos, 2, "Archive Info")?;
                let len = usize::from(read_u16(data, pos));
                pos += 2 + len;
            }
            0x33 => {
                check_len(data, pos, 1, "Hardware Type")?;
                let count = usize::from(data[pos]);
                pos += 1 + count * 3;
            }
            0x35 => {
                check_len(data, pos, 20, "Custom Info")?;
                let len = read_u32(data, pos + 16) as usize;
                pos += 20 + len;
            }
            0x5A => {
                check_len(data, pos, 9, "Glue Block")?;
                pos += 9;
            }
            _ => {
                return Err(format!(
                    "Unknown TZX block ID 0x{:02X} at offset {}",
                    block_id,
                    pos - 1
                ));
            }
        }
    }

    Ok(pulses)
}

fn parse_standard_speed(data: &[u8], pos: usize, pulses: &mut Vec<u32>) -> Result<usize, String> {
    check_len(data, pos, 4, "Standard Speed")?;
    let pause_ms = u32::from(read_u16(data, pos));
    let data_len = usize::from(read_u16(data, pos + 2));
    let block_start = pos + 4;
    check_len(data, block_start, data_len, "Standard Speed data")?;

    let block_data = &data[block_start..block_start + data_len];
    let flag = block_data.first().copied().unwrap_or(0xFF);
    let pilot_count = if flag < 0x80 {
        PILOT_COUNT_HEADER
    } else {
        PILOT_COUNT_DATA
    };

    for _ in 0..pilot_count {
        pulses.push(PILOT_PULSE);
    }
    pulses.push(SYNC1_PULSE);
    pulses.push(SYNC2_PULSE);
    append_data_pulses(block_data, 8, ZERO_PULSE, ONE_PULSE, pulses);

    if pause_ms > 0 {
        pulses.push(pause_ms * TSTATES_PER_MS);
    }

    Ok(block_start + data_len)
}

fn parse_turbo_speed(data: &[u8], pos: usize, pulses: &mut Vec<u32>) -> Result<usize, String> {
    check_len(data, pos, 0x12, "Turbo Speed header")?;

    let pilot_len = u32::from(read_u16(data, pos));
    let sync1_len = u32::from(read_u16(data, pos + 2));
    let sync2_len = u32::from(read_u16(data, pos + 4));
    let zero_len = u32::from(read_u16(data, pos + 6));
    let one_len = u32::from(read_u16(data, pos + 8));
    let pilot_count = u32::from(read_u16(data, pos + 0x0A));
    let bits_last = data[pos + 0x0C];
    let pause_ms = u32::from(read_u16(data, pos + 0x0D));
    let data_len = read_u24(data, pos + 0x0F) as usize;
    let block_start = pos + 0x12;
    check_len(data, block_start, data_len, "Turbo Speed data")?;

    let block_data = &data[block_start..block_start + data_len];

    for _ in 0..pilot_count {
        pulses.push(pilot_len);
    }
    if sync1_len > 0 {
        pulses.push(sync1_len);
    }
    if sync2_len > 0 {
        pulses.push(sync2_len);
    }

    append_data_pulses(block_data, bits_last, zero_len, one_len, pulses);

    if pause_ms > 0 {
        pulses.push(pause_ms * TSTATES_PER_MS);
    }

    Ok(block_start + data_len)
}

fn parse_pure_tone(data: &[u8], pos: usize, pulses: &mut Vec<u32>) -> Result<usize, String> {
    check_len(data, pos, 4, "Pure Tone")?;
    let pulse_len = u32::from(read_u16(data, pos));
    let count = u32::from(read_u16(data, pos + 2));

    for _ in 0..count {
        pulses.push(pulse_len);
    }

    Ok(pos + 4)
}

fn parse_pulse_sequence(data: &[u8], pos: usize, pulses: &mut Vec<u32>) -> Result<usize, String> {
    check_len(data, pos, 1, "Pulse Sequence")?;
    let count = usize::from(data[pos]);
    check_len(data, pos + 1, count * 2, "Pulse Sequence data")?;

    for index in 0..count {
        let pulse_len = u32::from(read_u16(data, pos + 1 + index * 2));
        pulses.push(pulse_len);
    }

    Ok(pos + 1 + count * 2)
}

fn parse_pure_data(data: &[u8], pos: usize, pulses: &mut Vec<u32>) -> Result<usize, String> {
    check_len(data, pos, 0x0A, "Pure Data header")?;

    let zero_len = u32::from(read_u16(data, pos));
    let one_len = u32::from(read_u16(data, pos + 2));
    let bits_last = data[pos + 4];
    let pause_ms = u32::from(read_u16(data, pos + 5));
    let data_len = read_u24(data, pos + 7) as usize;
    let block_start = pos + 0x0A;
    check_len(data, block_start, data_len, "Pure Data data")?;

    let block_data = &data[block_start..block_start + data_len];
    append_data_pulses(block_data, bits_last, zero_len, one_len, pulses);

    if pause_ms > 0 {
        pulses.push(pause_ms * TSTATES_PER_MS);
    }

    Ok(block_start + data_len)
}

fn parse_direct_recording(data: &[u8], pos: usize, pulses: &mut Vec<u32>) -> Result<usize, String> {
    check_len(data, pos, 8, "Direct Recording header")?;

    let tstates_per_sample = u32::from(read_u16(data, pos));
    let pause_ms = u32::from(read_u16(data, pos + 2));
    let bits_last = data[pos + 4];
    let data_len = read_u24(data, pos + 5) as usize;
    let block_start = pos + 8;
    check_len(data, block_start, data_len, "Direct Recording data")?;

    let block_data = &data[block_start..block_start + data_len];
    let mut current_level: Option<bool> = None;
    let mut run_length = 0u32;

    let last_byte_idx = data_len.saturating_sub(1);
    for (byte_idx, &byte) in block_data.iter().enumerate() {
        let bits = if byte_idx == last_byte_idx {
            bits_last
        } else {
            8
        };

        for bit_pos in (0..bits).rev() {
            let sample = byte & (1 << bit_pos) != 0;
            match current_level {
                None => {
                    current_level = Some(sample);
                    run_length = tstates_per_sample;
                }
                Some(level) if level == sample => {
                    run_length += tstates_per_sample;
                }
                Some(_) => {
                    pulses.push(run_length);
                    current_level = Some(sample);
                    run_length = tstates_per_sample;
                }
            }
        }
    }

    if run_length > 0 {
        pulses.push(run_length);
    }

    if pause_ms > 0 {
        pulses.push(pause_ms * TSTATES_PER_MS);
    }

    Ok(block_start + data_len)
}

fn parse_pause(data: &[u8], pos: usize, pulses: &mut Vec<u32>) -> Result<usize, String> {
    check_len(data, pos, 2, "Pause")?;
    let pause_ms = u32::from(read_u16(data, pos));
    if pause_ms > 0 {
        pulses.push(pause_ms * TSTATES_PER_MS);
    }
    Ok(pos + 2)
}

fn append_data_pulses(
    data: &[u8],
    bits_in_last_byte: u8,
    zero_len: u32,
    one_len: u32,
    pulses: &mut Vec<u32>,
) {
    if data.is_empty() {
        return;
    }

    let last_idx = data.len() - 1;
    for (idx, &byte) in data.iter().enumerate() {
        let bits = if idx == last_idx {
            bits_in_last_byte
        } else {
            8
        };
        for bit in (0..bits).rev() {
            let pulse = if byte & (1 << bit) != 0 {
                one_len
            } else {
                zero_len
            };
            pulses.push(pulse);
            pulses.push(pulse);
        }
    }
}

fn read_u16(data: &[u8], pos: usize) -> u16 {
    u16::from_le_bytes([data[pos], data[pos + 1]])
}

fn read_u24(data: &[u8], pos: usize) -> u32 {
    u32::from_le_bytes([data[pos], data[pos + 1], data[pos + 2], 0])
}

fn read_u32(data: &[u8], pos: usize) -> u32 {
    u32::from_le_bytes([data[pos], data[pos + 1], data[pos + 2], data[pos + 3]])
}

fn check_len(data: &[u8], pos: usize, need: usize, ctx: &str) -> Result<(), String> {
    if pos + need > data.len() {
        Err(format!(
            "TZX {}: need {} bytes at offset {}, but file is only {} bytes",
            ctx,
            need,
            pos,
            data.len()
        ))
    } else {
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn make_header() -> Vec<u8> {
        let mut header = b"ZXTape!\x1a".to_vec();
        header.push(1);
        header.push(20);
        header
    }

    #[test]
    fn empty_tzx_has_no_pulses() {
        let pulses = tzx_to_pulses(&make_header()).expect("empty header should parse");
        assert!(pulses.is_empty());
    }

    #[test]
    fn pure_tone_block_parses() {
        let mut data = make_header();
        data.push(0x12);
        data.extend_from_slice(&500u16.to_le_bytes());
        data.extend_from_slice(&10u16.to_le_bytes());

        let pulses = tzx_to_pulses(&data).expect("pure tone should parse");
        assert_eq!(pulses.len(), 10);
        assert!(pulses.iter().all(|&pulse| pulse == 500));
    }

    #[test]
    fn pulse_sequence_block_parses() {
        let mut data = make_header();
        data.push(0x13);
        data.push(3);
        data.extend_from_slice(&100u16.to_le_bytes());
        data.extend_from_slice(&200u16.to_le_bytes());
        data.extend_from_slice(&300u16.to_le_bytes());

        let pulses = tzx_to_pulses(&data).expect("pulse sequence should parse");
        assert_eq!(pulses, vec![100, 200, 300]);
    }

    #[test]
    fn standard_speed_block_expands_rom_timing() {
        let mut data = make_header();
        data.push(0x10);
        data.extend_from_slice(&1_000u16.to_le_bytes());
        data.extend_from_slice(&2u16.to_le_bytes());
        data.push(0xFF);
        data.push(0x00);

        let pulses = tzx_to_pulses(&data).expect("standard-speed block should parse");
        let expected_len = PILOT_COUNT_DATA as usize + 2 + 32 + 1;
        assert_eq!(pulses.len(), expected_len);
        assert_eq!(pulses[0], PILOT_PULSE);
    }

    #[test]
    fn metadata_blocks_are_skipped() {
        let mut data = make_header();
        data.push(0x30);
        data.push(5);
        data.extend_from_slice(b"Hello");
        data.push(0x21);
        data.push(4);
        data.extend_from_slice(b"Test");
        data.push(0x22);
        data.push(0x12);
        data.extend_from_slice(&100u16.to_le_bytes());
        data.extend_from_slice(&1u16.to_le_bytes());

        let pulses = tzx_to_pulses(&data).expect("metadata-skipping parse should succeed");
        assert_eq!(pulses, vec![100]);
    }

    #[test]
    fn loop_block_expands_body() {
        let mut data = make_header();
        data.push(0x24);
        data.extend_from_slice(&3u16.to_le_bytes());
        data.push(0x12);
        data.extend_from_slice(&100u16.to_le_bytes());
        data.extend_from_slice(&2u16.to_le_bytes());
        data.push(0x25);

        let pulses = tzx_to_pulses(&data).expect("loop expansion should parse");
        assert_eq!(pulses.len(), 6);
        assert!(pulses.iter().all(|&pulse| pulse == 100));
    }

    #[test]
    fn invalid_header_is_rejected() {
        let err = tzx_to_pulses(b"NOT_TZX!").expect_err("bad header should fail");
        assert!(err.contains("bad header") || err.contains("too short"));
    }
}
