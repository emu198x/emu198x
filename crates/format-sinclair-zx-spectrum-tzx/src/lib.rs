//! TZX tape format parser for the Sinclair ZX Spectrum.
//!
//! Source references:
//! - `knowledge/concepts/tape-formats.md`
//! - Adapted from `../Emu198x-Older/crates/format-sinclair-zx-spectrum-tzx/src/lib.rs`
//!
//! The parser converts TZX blocks into a flat machine-facing timing stream.
//! Most blocks emit edge-delimited pulses, but pauses and explicit signal-level
//! directives require held-level spans as well.

use common_sinclair_zx_spectrum::TapeSpan;

const PILOT_PULSE: u32 = 2_168;
const SYNC1_PULSE: u32 = 667;
const SYNC2_PULSE: u32 = 735;
const ZERO_PULSE: u32 = 855;
const ONE_PULSE: u32 = 1_710;
const PILOT_COUNT_HEADER: u32 = 8_063;
const PILOT_COUNT_DATA: u32 = 3_223;
const TSTATES_PER_MS: u32 = 3_500;

/// Parses a TZX file and returns one machine-facing timing stream.
///
/// # Errors
///
/// Returns an error if the file header is invalid, the version is unsupported,
/// a block overruns the supplied bytes, or the file contains an unknown block.
pub fn tzx_to_stream(data: &[u8]) -> Result<Vec<TapeSpan>, String> {
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

    let mut stream = Vec::new();
    let mut pos = 10usize;
    let mut current_level = false;
    let mut loop_start_stream_len = 0usize;
    let mut loop_remaining = 0u16;

    while pos < data.len() {
        let block_id = data[pos];
        pos += 1;

        match block_id {
            0x10 => pos = parse_standard_speed(data, pos, &mut current_level, &mut stream)?,
            0x11 => pos = parse_turbo_speed(data, pos, &mut current_level, &mut stream)?,
            0x12 => pos = parse_pure_tone(data, pos, &mut current_level, &mut stream)?,
            0x13 => pos = parse_pulse_sequence(data, pos, &mut current_level, &mut stream)?,
            0x14 => pos = parse_pure_data(data, pos, &mut current_level, &mut stream)?,
            0x15 => pos = parse_direct_recording(data, pos, &mut current_level, &mut stream)?,
            0x20 => pos = parse_pause(data, pos, &mut current_level, &mut stream)?,
            0x24 => {
                check_len(data, pos, 2, "Loop Start")?;
                loop_remaining = read_u16(data, pos);
                loop_start_stream_len = stream.len();
                pos += 2;
            }
            0x25 => {
                if loop_remaining > 1 {
                    let body = stream[loop_start_stream_len..].to_vec();
                    for _ in 1..loop_remaining {
                        stream.extend_from_slice(&body);
                    }
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
                let payload_start = pos + 4;
                check_len(data, payload_start, len, "Set Signal Level data")?;
                if let Some(&level) = data[payload_start..payload_start + len].last() {
                    current_level = level != 0;
                    stream.push(TapeSpan::Level {
                        duration: 0,
                        level: current_level,
                    });
                }
                pos = payload_start + len;
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

    Ok(stream)
}

fn parse_standard_speed(
    data: &[u8],
    pos: usize,
    current_level: &mut bool,
    stream: &mut Vec<TapeSpan>,
) -> Result<usize, String> {
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
        push_pulse(current_level, PILOT_PULSE, stream);
    }
    push_pulse(current_level, SYNC1_PULSE, stream);
    push_pulse(current_level, SYNC2_PULSE, stream);
    append_data_spans(block_data, 8, ZERO_PULSE, ONE_PULSE, current_level, stream);
    append_pause_spans(pause_ms, current_level, stream);

    Ok(block_start + data_len)
}

fn parse_turbo_speed(
    data: &[u8],
    pos: usize,
    current_level: &mut bool,
    stream: &mut Vec<TapeSpan>,
) -> Result<usize, String> {
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
        push_pulse(current_level, pilot_len, stream);
    }
    if sync1_len > 0 {
        push_pulse(current_level, sync1_len, stream);
    }
    if sync2_len > 0 {
        push_pulse(current_level, sync2_len, stream);
    }

    append_data_spans(
        block_data,
        bits_last,
        zero_len,
        one_len,
        current_level,
        stream,
    );
    append_pause_spans(pause_ms, current_level, stream);

    Ok(block_start + data_len)
}

fn parse_pure_tone(
    data: &[u8],
    pos: usize,
    current_level: &mut bool,
    stream: &mut Vec<TapeSpan>,
) -> Result<usize, String> {
    check_len(data, pos, 4, "Pure Tone")?;
    let pulse_len = u32::from(read_u16(data, pos));
    let count = u32::from(read_u16(data, pos + 2));

    for _ in 0..count {
        push_pulse(current_level, pulse_len, stream);
    }

    Ok(pos + 4)
}

fn parse_pulse_sequence(
    data: &[u8],
    pos: usize,
    current_level: &mut bool,
    stream: &mut Vec<TapeSpan>,
) -> Result<usize, String> {
    check_len(data, pos, 1, "Pulse Sequence")?;
    let count = usize::from(data[pos]);
    check_len(data, pos + 1, count * 2, "Pulse Sequence data")?;

    for index in 0..count {
        let pulse_len = u32::from(read_u16(data, pos + 1 + index * 2));
        push_pulse(current_level, pulse_len, stream);
    }

    Ok(pos + 1 + count * 2)
}

fn parse_pure_data(
    data: &[u8],
    pos: usize,
    current_level: &mut bool,
    stream: &mut Vec<TapeSpan>,
) -> Result<usize, String> {
    check_len(data, pos, 0x0A, "Pure Data header")?;

    let zero_len = u32::from(read_u16(data, pos));
    let one_len = u32::from(read_u16(data, pos + 2));
    let bits_last = data[pos + 4];
    let pause_ms = u32::from(read_u16(data, pos + 5));
    let data_len = read_u24(data, pos + 7) as usize;
    let block_start = pos + 0x0A;
    check_len(data, block_start, data_len, "Pure Data data")?;

    let block_data = &data[block_start..block_start + data_len];
    append_data_spans(
        block_data,
        bits_last,
        zero_len,
        one_len,
        current_level,
        stream,
    );
    append_pause_spans(pause_ms, current_level, stream);

    Ok(block_start + data_len)
}

fn parse_direct_recording(
    data: &[u8],
    pos: usize,
    current_level: &mut bool,
    stream: &mut Vec<TapeSpan>,
) -> Result<usize, String> {
    check_len(data, pos, 8, "Direct Recording header")?;

    let tstates_per_sample = u32::from(read_u16(data, pos));
    let pause_ms = u32::from(read_u16(data, pos + 2));
    let bits_last = data[pos + 4];
    let data_len = read_u24(data, pos + 5) as usize;
    let block_start = pos + 8;
    check_len(data, block_start, data_len, "Direct Recording data")?;

    let block_data = &data[block_start..block_start + data_len];
    let mut run_level: Option<bool> = None;
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
            match run_level {
                None => {
                    run_level = Some(sample);
                    run_length = tstates_per_sample;
                }
                Some(level) if level == sample => {
                    run_length += tstates_per_sample;
                }
                Some(level) => {
                    stream.push(TapeSpan::Level {
                        duration: run_length,
                        level,
                    });
                    run_level = Some(sample);
                    run_length = tstates_per_sample;
                }
            }
        }
    }

    if let Some(level) = run_level {
        if run_length > 0 {
            stream.push(TapeSpan::Level {
                duration: run_length,
                level,
            });
        }
        *current_level = level;
    }

    append_pause_spans(pause_ms, current_level, stream);
    Ok(block_start + data_len)
}

fn parse_pause(
    data: &[u8],
    pos: usize,
    current_level: &mut bool,
    stream: &mut Vec<TapeSpan>,
) -> Result<usize, String> {
    check_len(data, pos, 2, "Pause")?;
    let pause_ms = u32::from(read_u16(data, pos));
    // The standalone Pause block (0x20) treats pause=0 as "stop the
    // tape and wait for explicit resume" — distinct from the
    // pause-after-data field in 0x10/0x11/0x14/0x15, where 0 means
    // "no pause, continue immediately". `append_pause_spans` handles
    // only the latter; we emit the Stop signal here.
    if pause_ms == 0 {
        stream.push(TapeSpan::Stop);
    } else {
        append_pause_spans(pause_ms, current_level, stream);
    }
    Ok(pos + 2)
}

fn append_data_spans(
    data: &[u8],
    bits_in_last_byte: u8,
    zero_len: u32,
    one_len: u32,
    current_level: &mut bool,
    stream: &mut Vec<TapeSpan>,
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
        // The TZX spec stores partial last bytes left-justified: when
        // `bits_in_last_byte` is N < 8, the N significant bits live in
        // the upper N bits of the byte (bits 7..8-N) and the lower 8-N
        // bits are zero. Iterate the upper bits MSB-first.
        for bit in (8 - bits..8).rev() {
            let pulse = if byte & (1 << bit) != 0 {
                one_len
            } else {
                zero_len
            };
            push_pulse(current_level, pulse, stream);
            push_pulse(current_level, pulse, stream);
        }
    }
}

/// Pause-after-data emission for the data-bearing TZX blocks
/// (0x10 / 0x11 / 0x14 / 0x15). The TZX spec defines pause=0 in
/// those contexts as "no pause, continue immediately to the next
/// block" — *not* as a stop signal. Speedlock 7 tapes chain
/// dozens of pure-data blocks back-to-back via pause=0; misreading
/// that as a Stop causes `tape.is_playing` to flip false mid-load
/// and any catalogue runner that waits-for-tape-stop returns long
/// before the loader has finished.
///
/// The standalone Pause block (0x20) has different semantics
/// (pause=0 means "stop the tape") and emits a Stop directly from
/// `parse_pause` rather than going through this function.
fn append_pause_spans(pause_ms: u32, current_level: &mut bool, stream: &mut Vec<TapeSpan>) {
    if pause_ms == 0 {
        return;
    }

    stream.push(TapeSpan::Level {
        duration: TSTATES_PER_MS,
        level: *current_level,
    });

    if pause_ms > 1 {
        stream.push(TapeSpan::Level {
            duration: (pause_ms - 1) * TSTATES_PER_MS,
            level: false,
        });
        *current_level = false;
    }
}

fn push_pulse(current_level: &mut bool, duration: u32, stream: &mut Vec<TapeSpan>) {
    if duration == 0 {
        *current_level = !*current_level;
        return;
    }

    stream.push(TapeSpan::Pulse(duration));
    *current_level = !*current_level;
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
    fn empty_tzx_has_no_spans() {
        let stream = tzx_to_stream(&make_header()).expect("empty header should parse");
        assert!(stream.is_empty());
    }

    #[test]
    fn pure_tone_block_parses() {
        let mut data = make_header();
        data.push(0x12);
        data.extend_from_slice(&500u16.to_le_bytes());
        data.extend_from_slice(&10u16.to_le_bytes());

        let stream = tzx_to_stream(&data).expect("pure tone should parse");
        assert_eq!(stream, vec![TapeSpan::Pulse(500); 10]);
    }

    #[test]
    fn pulse_sequence_block_parses() {
        let mut data = make_header();
        data.push(0x13);
        data.push(3);
        data.extend_from_slice(&100u16.to_le_bytes());
        data.extend_from_slice(&200u16.to_le_bytes());
        data.extend_from_slice(&300u16.to_le_bytes());

        let stream = tzx_to_stream(&data).expect("pulse sequence should parse");
        assert_eq!(
            stream,
            vec![
                TapeSpan::Pulse(100),
                TapeSpan::Pulse(200),
                TapeSpan::Pulse(300)
            ]
        );
    }

    #[test]
    fn standard_speed_block_appends_pause_level_holds() {
        let mut data = make_header();
        data.push(0x10);
        data.extend_from_slice(&1_000u16.to_le_bytes());
        data.extend_from_slice(&2u16.to_le_bytes());
        data.extend_from_slice(&[0x00, 0xff]);

        let stream = tzx_to_stream(&data).expect("standard-speed block should parse");
        assert_eq!(stream.first(), Some(&TapeSpan::Pulse(PILOT_PULSE)));
        assert_eq!(
            stream.last(),
            Some(&TapeSpan::Level {
                duration: 999 * TSTATES_PER_MS,
                level: false,
            })
        );
    }

    #[test]
    fn metadata_blocks_are_skipped() {
        let mut data = make_header();
        data.push(0x30);
        data.push(4);
        data.extend_from_slice(b"demo");
        data.push(0x12);
        data.extend_from_slice(&250u16.to_le_bytes());
        data.extend_from_slice(&2u16.to_le_bytes());

        let stream = tzx_to_stream(&data).expect("metadata-skipping parse should succeed");
        assert_eq!(stream, vec![TapeSpan::Pulse(250), TapeSpan::Pulse(250)]);
    }

    #[test]
    fn loop_block_expands_body() {
        let mut data = make_header();
        data.push(0x24);
        data.extend_from_slice(&3u16.to_le_bytes());
        data.push(0x12);
        data.extend_from_slice(&100u16.to_le_bytes());
        data.extend_from_slice(&1u16.to_le_bytes());
        data.push(0x25);

        let stream = tzx_to_stream(&data).expect("loop expansion should parse");
        assert_eq!(stream, vec![TapeSpan::Pulse(100); 3]);
    }

    #[test]
    fn direct_recording_parses_to_level_holds() {
        let mut data = make_header();
        data.push(0x15);
        data.extend_from_slice(&10u16.to_le_bytes());
        data.extend_from_slice(&0u16.to_le_bytes());
        data.push(8);
        data.extend_from_slice(&1u32.to_le_bytes()[..3]);
        data.push(0b1111_0000);

        // pause=0 after the direct-recording data means "no pause,
        // continue to next block" per the TZX spec, so the stream
        // should end after the recorded level holds with no Stop.
        let stream = tzx_to_stream(&data).expect("direct recording should parse");
        assert_eq!(
            stream,
            vec![
                TapeSpan::Level {
                    duration: 40,
                    level: true,
                },
                TapeSpan::Level {
                    duration: 40,
                    level: false,
                },
            ]
        );
    }

    #[test]
    fn pause_zero_stops_playback() {
        let mut data = make_header();
        data.push(0x20);
        data.extend_from_slice(&0u16.to_le_bytes());

        let stream = tzx_to_stream(&data).expect("pause block should parse");
        assert_eq!(stream, vec![TapeSpan::Stop]);
    }

    #[test]
    fn set_signal_level_emits_zero_length_level_span() {
        let mut data = make_header();
        data.push(0x2B);
        data.extend_from_slice(&1u32.to_le_bytes());
        data.push(1);

        let stream = tzx_to_stream(&data).expect("set signal level should parse");
        assert_eq!(
            stream,
            vec![TapeSpan::Level {
                duration: 0,
                level: true,
            }]
        );
    }

    #[test]
    fn invalid_header_is_rejected() {
        let err = tzx_to_stream(b"NOT_TZX!!\x00\x00").expect_err("bad header should fail");
        assert!(err.contains("bad header"));
    }

    /// 0x11 Turbo Speed Data: like 0x10 but with caller-specified
    /// pilot/sync/zero/one pulse lengths and pilot tone count. Used by
    /// custom loaders. Tests the full parameter chain.
    #[test]
    fn turbo_speed_block_uses_custom_pulses() {
        let mut data = make_header();
        data.push(0x11);
        // Pilot pulse (u16), sync1 (u16), sync2 (u16), zero (u16),
        // one (u16), pilot count (u16), bits_in_last_byte (u8),
        // pause (u16), data_len (u24), data...
        data.extend_from_slice(&1000u16.to_le_bytes()); // pilot pulse
        data.extend_from_slice(&500u16.to_le_bytes()); // sync 1
        data.extend_from_slice(&600u16.to_le_bytes()); // sync 2
        data.extend_from_slice(&100u16.to_le_bytes()); // zero
        data.extend_from_slice(&200u16.to_le_bytes()); // one
        data.extend_from_slice(&3u16.to_le_bytes()); // 3 pilot pulses
        data.push(8); // 8 bits in last byte
        data.extend_from_slice(&0u16.to_le_bytes()); // pause = 0 → no stop
        data.extend_from_slice(&[1u8, 0, 0]); // data_len = 1 (u24)
        data.push(0b1010_0000); // bits: 1 0 1 0 0 0 0 0

        let stream = tzx_to_stream(&data).expect("turbo speed block should parse");
        // Expected: 3 pilot pulses (1000 each), sync1 (500), sync2 (600),
        // then 8 data bits: 1→(200,200), 0→(100,100), 1→(200,200),
        // 0→(100,100), 0,0,0,0 → all (100,100).
        let expected: Vec<TapeSpan> = std::iter::repeat_n(TapeSpan::Pulse(1000), 3)
            .chain(std::iter::once(TapeSpan::Pulse(500)))
            .chain(std::iter::once(TapeSpan::Pulse(600)))
            .chain(
                [
                    200, 200, 100, 100, 200, 200, 100, 100, 100, 100, 100, 100, 100, 100, 100, 100,
                ]
                .into_iter()
                .map(TapeSpan::Pulse),
            )
            .collect();
        assert_eq!(stream, expected);
    }

    /// 0x20 Pause block with non-zero duration emits a level-hold
    /// pair: 1 ms at the current level (preserving the trailing level
    /// of the previous block) followed by (pause - 1) ms at the low
    /// level. Distinct from the pause=0 case which emits Stop.
    #[test]
    fn pause_nonzero_emits_level_hold() {
        let mut data = make_header();
        data.push(0x20);
        data.extend_from_slice(&5u16.to_le_bytes()); // 5 ms pause

        let stream = tzx_to_stream(&data).expect("pause block should parse");
        assert_eq!(stream.len(), 2, "5 ms pause emits a 1 ms + 4 ms pair");
        // First span: 1 ms at the current level (false at fresh start).
        assert_eq!(
            stream[0],
            TapeSpan::Level {
                duration: TSTATES_PER_MS,
                level: false,
            }
        );
        // Second span: (pause - 1) ms low.
        assert_eq!(
            stream[1],
            TapeSpan::Level {
                duration: 4 * TSTATES_PER_MS,
                level: false,
            }
        );
    }

    /// 0x20 Pause block with exactly 1 ms emits just the level-preserving
    /// span (no trailing low-level hold since pause - 1 == 0).
    #[test]
    fn pause_one_ms_emits_single_level_hold() {
        let mut data = make_header();
        data.push(0x20);
        data.extend_from_slice(&1u16.to_le_bytes()); // 1 ms pause

        let stream = tzx_to_stream(&data).expect("1 ms pause should parse");
        assert_eq!(stream.len(), 1);
        assert_eq!(
            stream[0],
            TapeSpan::Level {
                duration: TSTATES_PER_MS,
                level: false,
            }
        );
    }

    /// 0x21 Group Start: contains a 1-byte name length followed by the
    /// name. Parser skips both — group bracketing is purely advisory.
    #[test]
    fn group_start_skips_name_payload() {
        let mut data = make_header();
        data.push(0x21);
        data.push(5); // 5-byte name
        data.extend_from_slice(b"intro");
        // A pure-tone block after the group-start should still parse.
        data.push(0x12);
        data.extend_from_slice(&100u16.to_le_bytes());
        data.extend_from_slice(&1u16.to_le_bytes());

        let stream = tzx_to_stream(&data).expect("group start should skip");
        assert_eq!(stream, vec![TapeSpan::Pulse(100)]);
    }

    /// 0x22 Group End: zero-payload, no-op for the parser.
    #[test]
    fn group_end_is_a_no_op() {
        let mut data = make_header();
        data.push(0x22);
        // Trailing pure tone proves the parser advanced correctly.
        data.push(0x12);
        data.extend_from_slice(&250u16.to_le_bytes());
        data.extend_from_slice(&1u16.to_le_bytes());

        let stream = tzx_to_stream(&data).expect("group end should parse");
        assert_eq!(stream, vec![TapeSpan::Pulse(250)]);
    }

    /// 0x23 Jump To Block: 2-byte signed offset. Parser skips it
    /// (our implementation doesn't actually follow jumps — see the
    /// loop / call macros above).
    #[test]
    fn jump_to_block_skips_offset() {
        let mut data = make_header();
        data.push(0x23);
        data.extend_from_slice(&0u16.to_le_bytes());
        data.push(0x12);
        data.extend_from_slice(&123u16.to_le_bytes());
        data.extend_from_slice(&1u16.to_le_bytes());

        let stream = tzx_to_stream(&data).expect("jump block should parse");
        assert_eq!(stream, vec![TapeSpan::Pulse(123)]);
    }

    /// 0x26 Call Sequence: 2-byte count + 2 bytes per call entry.
    /// Parser advances past the table without taking the calls.
    #[test]
    fn call_sequence_skips_call_table() {
        let mut data = make_header();
        data.push(0x26);
        data.extend_from_slice(&3u16.to_le_bytes()); // 3 calls
        data.extend_from_slice(&0u16.to_le_bytes());
        data.extend_from_slice(&0u16.to_le_bytes());
        data.extend_from_slice(&0u16.to_le_bytes());
        // Follow-up tone proves the parser advanced correctly.
        data.push(0x12);
        data.extend_from_slice(&77u16.to_le_bytes());
        data.extend_from_slice(&1u16.to_le_bytes());

        let stream = tzx_to_stream(&data).expect("call sequence should parse");
        assert_eq!(stream, vec![TapeSpan::Pulse(77)]);
    }

    /// 0x27 Return: zero-payload, no-op (we don't follow calls).
    #[test]
    fn return_block_is_a_no_op() {
        let mut data = make_header();
        data.push(0x27);
        data.push(0x12);
        data.extend_from_slice(&88u16.to_le_bytes());
        data.extend_from_slice(&1u16.to_le_bytes());

        let stream = tzx_to_stream(&data).expect("return block should parse");
        assert_eq!(stream, vec![TapeSpan::Pulse(88)]);
    }

    /// 0x28 Select Block: 2-byte length, then a payload of options.
    /// Parser skips the whole payload without picking a branch.
    #[test]
    fn select_block_skips_payload() {
        let mut data = make_header();
        data.push(0x28);
        data.extend_from_slice(&4u16.to_le_bytes()); // 4-byte payload
        data.extend_from_slice(&[0, 0, 0, 0]);
        data.push(0x12);
        data.extend_from_slice(&99u16.to_le_bytes());
        data.extend_from_slice(&1u16.to_le_bytes());

        let stream = tzx_to_stream(&data).expect("select block should parse");
        assert_eq!(stream, vec![TapeSpan::Pulse(99)]);
    }

    /// 0x2A Stop The Tape If 48K: 4-byte length + payload. The parser
    /// skips the whole thing in the current implementation (no Stop
    /// emitted — the TZX spec says emit Stop only on a 48K machine,
    /// and the runtime layer is the right place to make that
    /// decision).
    #[test]
    fn stop_48k_block_skips_payload() {
        let mut data = make_header();
        data.push(0x2A);
        data.extend_from_slice(&0u32.to_le_bytes()); // zero-length payload
        data.push(0x12);
        data.extend_from_slice(&55u16.to_le_bytes());
        data.extend_from_slice(&1u16.to_le_bytes());

        let stream = tzx_to_stream(&data).expect("stop-48K block should parse");
        assert_eq!(stream, vec![TapeSpan::Pulse(55)]);
    }

    /// 0x31 Message Block: 1-byte display time + 1-byte length + text.
    /// Pure metadata; the parser skips the whole payload.
    #[test]
    fn message_block_skips_payload() {
        let mut data = make_header();
        data.push(0x31);
        data.push(5); // display 5 seconds
        data.push(4); // 4-byte text
        data.extend_from_slice(b"info");
        data.push(0x12);
        data.extend_from_slice(&44u16.to_le_bytes());
        data.extend_from_slice(&1u16.to_le_bytes());

        let stream = tzx_to_stream(&data).expect("message block should parse");
        assert_eq!(stream, vec![TapeSpan::Pulse(44)]);
    }

    /// 0x32 Archive Info: 2-byte length + n×(1-byte ID + 1-byte
    /// length + text) records. Pure metadata, parser skips it.
    #[test]
    fn archive_info_skips_payload() {
        let mut data = make_header();
        data.push(0x32);
        data.extend_from_slice(&6u16.to_le_bytes()); // 6-byte payload
        data.extend_from_slice(b"\x00\x04test");
        data.push(0x12);
        data.extend_from_slice(&66u16.to_le_bytes());
        data.extend_from_slice(&1u16.to_le_bytes());

        let stream = tzx_to_stream(&data).expect("archive info should parse");
        assert_eq!(stream, vec![TapeSpan::Pulse(66)]);
    }

    /// 0x33 Hardware Type: 1-byte count + count × (3 bytes per entry).
    /// Pure metadata, parser skips it.
    #[test]
    fn hardware_type_skips_payload() {
        let mut data = make_header();
        data.push(0x33);
        data.push(2); // 2 entries
        data.extend_from_slice(&[0x00, 0x00, 0x00, 0x01, 0x00, 0x00]);
        data.push(0x12);
        data.extend_from_slice(&33u16.to_le_bytes());
        data.extend_from_slice(&1u16.to_le_bytes());

        let stream = tzx_to_stream(&data).expect("hardware type should parse");
        assert_eq!(stream, vec![TapeSpan::Pulse(33)]);
    }

    /// 0x35 Custom Info: 16-byte identifier + 4-byte length + payload.
    /// Pure metadata, parser skips it.
    #[test]
    fn custom_info_skips_payload() {
        let mut data = make_header();
        data.push(0x35);
        data.extend_from_slice(&[0u8; 16]); // 16-byte ID
        data.extend_from_slice(&4u32.to_le_bytes()); // 4-byte payload
        data.extend_from_slice(b"abcd");
        data.push(0x12);
        data.extend_from_slice(&22u16.to_le_bytes());
        data.extend_from_slice(&1u16.to_le_bytes());

        let stream = tzx_to_stream(&data).expect("custom info should parse");
        assert_eq!(stream, vec![TapeSpan::Pulse(22)]);
    }

    /// 0x5A Glue Block: 9-byte zero-padding marker used by tools
    /// concatenating multiple TZX files. Parser skips it.
    #[test]
    fn glue_block_skips_payload() {
        let mut data = make_header();
        data.push(0x5A);
        data.extend_from_slice(&[0u8; 9]); // 9-byte padding
        data.push(0x12);
        data.extend_from_slice(&11u16.to_le_bytes());
        data.extend_from_slice(&1u16.to_le_bytes());

        let stream = tzx_to_stream(&data).expect("glue block should parse");
        assert_eq!(stream, vec![TapeSpan::Pulse(11)]);
    }

    /// Unknown TZX block IDs (anything outside the documented set)
    /// surface as an error including the offending byte. Catches a
    /// regression where the parser silently swallows unknown blocks.
    #[test]
    fn unknown_block_id_reports_offset() {
        let mut data = make_header();
        data.push(0xFE); // not a defined TZX block ID

        let err = tzx_to_stream(&data).expect_err("unknown block should error");
        assert!(err.contains("Unknown TZX block"), "got {err:?}");
        assert!(
            err.contains("0xFE"),
            "error should name the bad byte: {err:?}"
        );
    }

    /// A file shorter than 10 bytes (the minimum header length) is
    /// rejected with a "too short" message before any header check.
    #[test]
    fn file_shorter_than_header_rejected() {
        let err = tzx_to_stream(b"ZXTape!").expect_err("short file should fail");
        assert!(err.contains("too short"), "got {err:?}");
    }

    /// Major version > 1 is rejected — we only know how to parse
    /// TZX 1.x today. The error includes both major and minor for
    /// diagnostics.
    #[test]
    fn unsupported_major_version_rejected() {
        let mut data = b"ZXTape!\x1a".to_vec();
        data.push(2); // major
        data.push(0); // minor
        let err = tzx_to_stream(&data).expect_err("v2.0 should fail");
        assert!(err.contains("Unsupported"), "got {err:?}");
        assert!(
            err.contains("2.0"),
            "error should name the version: {err:?}"
        );
    }

    #[test]
    fn pure_data_partial_last_byte_uses_upper_bits() {
        // Regression: Speedlock-7 (Op Wolf et al.) uses a single-byte
        // 0x14 pure-data block with bits_in_last_byte=6 to deliver
        // the loader's check pattern. Per the TZX spec the N
        // significant bits live in the UPPER N bits of the last byte;
        // the parser previously read the LOWER N bits, producing the
        // wrong pulse sequence and tripping the Speedlock anti-tamper
        // wipe.
        //
        // Byte $E8 = 1110 1000. Top 6 bits = `1 1 1 0 1 0`.
        // With zero_len=10 and one_len=20, the expected stream is
        // `LL LL LL SS LL SS` = pulses [20,20, 20,20, 20,20, 10,10, 20,20, 10,10].
        let mut data = make_header();
        data.push(0x14); // 0x14 Pure Data
        data.extend_from_slice(&10u16.to_le_bytes()); // zero_len
        data.extend_from_slice(&20u16.to_le_bytes()); // one_len
        data.push(6); // bits_in_last_byte
        data.extend_from_slice(&0u16.to_le_bytes()); // pause
        data.extend_from_slice(&[1, 0, 0]); // data_len=1 (u24)
        data.push(0xE8);

        let stream = tzx_to_stream(&data).expect("pure data should parse");
        let expected: Vec<TapeSpan> = [20, 20, 20, 20, 20, 20, 10, 10, 20, 20, 10, 10]
            .into_iter()
            .map(TapeSpan::Pulse)
            .collect();
        assert_eq!(stream, expected);
    }
}
