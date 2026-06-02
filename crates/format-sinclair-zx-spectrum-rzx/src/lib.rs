//! RZX input recording format parser and writer.
//!
//! Adapted from `Emu198x-Older/crates/format-sinclair-zx-spectrum-rzx/`
//! during the 2026-06-01 Older harvest close-out — see
//! `knowledge/decisions/older-reference-only.md`.
//!
//! RZX is the input recording format developed by Ramsoft and used across
//! the Spectrum emulator scene (RealSpectrum, ZXSpin, Spectaculator,
//! ZEsarUX, …) to capture and replay sessions deterministically. The
//! format records every IN-port read with frame-accurate timing relative
//! to a starting snapshot, so a player given the same snapshot and the
//! same input stream produces the same output, every time.
//!
//! For Emu198x this matters in two ways:
//!
//! 1. **Replay regression testing.** Record a baseline session of an acid
//!    test (e.g. Signal Part 3) against a known-good build, then replay
//!    that RZX in CI on every PR and verify the resulting state hash. A
//!    subtle contention or AY regression that doesn't break boot tests
//!    will trip the replay.
//! 2. **Demoscene preservation.** Loading a finished demo session from a
//!    well-known archive lets users watch what was recorded without
//!    needing to rerun a tape load.
//!
//! ## Format overview
//!
//! - 10-byte header: `"RZX!"` magic + major + minor version + 4-byte flags
//! - Sequence of variable-length blocks, each `1 byte ID + 4 byte length + payload`
//! - Block 0x10: creator info (20-byte name + version)
//! - Block 0x30: snapshot (flags + extension + uncompressed length + data,
//!   with optional zlib compression of the snapshot data)
//! - Block 0x80: input recording (frame count + T-state counter + flags +
//!   frame stream, with optional zlib compression of the frame stream)
//! - Block 0x81/0x82: signature (not implemented — preserved as raw bytes
//!   if encountered)
//!
//! ## What this crate does and doesn't do
//!
//! Implements: header parsing, creator block, snapshot block (compressed
//! and uncompressed), input recording block (compressed and uncompressed),
//! frame stream including the `0xFFFF` "same as previous frame"
//! run-length-encoding marker. Round-trip writer.
//!
//! Doesn't implement: cryptographic signature verification, external
//! snapshot file references (the snapshot data is always embedded). The
//! parser will preserve unknown blocks as opaque payloads so they survive
//! round-trip without being interpreted.

use flate2::Compression;
use flate2::read::ZlibDecoder;
use flate2::write::ZlibEncoder;
use std::io::{Read, Write};

const MAGIC: &[u8; 4] = b"RZX!";

const BLOCK_CREATOR: u8 = 0x10;
const BLOCK_SNAPSHOT: u8 = 0x30;
const BLOCK_INPUT: u8 = 0x80;

const SNAPSHOT_FLAG_EXTERNAL: u32 = 0x01;
const SNAPSHOT_FLAG_COMPRESSED: u32 = 0x02;

const INPUT_FLAG_PROTECTED: u32 = 0x01;
const INPUT_FLAG_COMPRESSED: u32 = 0x02;

const SAME_AS_PREVIOUS: u16 = 0xFFFF;

/// A parsed RZX file. Round-trips through `write()` losslessly for the
/// block types we recognise; unknown blocks are preserved opaquely.
#[derive(Clone, Debug, Default)]
pub struct Rzx {
    pub version_major: u8,
    pub version_minor: u8,
    pub flags: u32,
    pub creator: Option<Creator>,
    pub snapshot: Option<Snapshot>,
    pub recordings: Vec<InputRecording>,
    /// Blocks the parser didn't recognise — preserved as `(id, payload)`
    /// pairs so the writer can re-emit them without interpretation.
    pub unknown_blocks: Vec<UnknownBlock>,
}

#[derive(Clone, Debug)]
pub struct Creator {
    /// Author/program name, padded to 20 bytes with NUL on disk.
    pub name: String,
    pub version_major: u16,
    pub version_minor: u16,
}

#[derive(Clone, Debug)]
pub struct Snapshot {
    pub flags: u32,
    /// File extension of the embedded snapshot, e.g. `"z80"` or `"sna"`.
    /// Stored as a 4-byte ASCII field on disk, NUL-padded.
    pub extension: String,
    /// Uncompressed snapshot bytes. The writer compresses on serialise if
    /// `flags & SNAPSHOT_FLAG_COMPRESSED` is set.
    pub data: Vec<u8>,
}

#[derive(Clone, Debug)]
pub struct InputRecording {
    pub flags: u32,
    /// T-state counter at the start of the recording. Some emulators
    /// store the absolute frame counter here; treat as opaque.
    pub tstate_counter: u32,
    pub frames: Vec<Frame>,
}

#[derive(Clone, Debug, PartialEq)]
pub struct Frame {
    /// Number of opcode fetches the CPU is expected to perform during
    /// this frame. Replay players use this to detect drift.
    pub fetch_count: u16,
    /// Bytes returned for each `IN` port read this frame, in order. The
    /// player feeds these to the CPU when the recorded number of fetches
    /// completes — *not* by reading the actual hardware.
    pub inputs: Vec<u8>,
}

#[derive(Clone, Debug)]
pub struct UnknownBlock {
    pub id: u8,
    /// Block payload (everything after the 5-byte header).
    pub payload: Vec<u8>,
}

/// Parse an RZX file from a byte slice.
pub fn parse(data: &[u8]) -> Result<Rzx, String> {
    if data.len() < 10 {
        return Err("RZX file shorter than 10-byte header".into());
    }
    if &data[..4] != MAGIC {
        return Err("Not an RZX file (missing RZX! magic)".into());
    }

    let mut rzx = Rzx {
        version_major: data[4],
        version_minor: data[5],
        flags: u32::from_le_bytes([data[6], data[7], data[8], data[9]]),
        ..Default::default()
    };

    let mut cursor = 10usize;
    while cursor < data.len() {
        if cursor + 5 > data.len() {
            return Err(format!("truncated block header at offset {}", cursor));
        }
        let block_id = data[cursor];
        let block_len = u32::from_le_bytes([
            data[cursor + 1],
            data[cursor + 2],
            data[cursor + 3],
            data[cursor + 4],
        ]) as usize;

        if block_len < 5 {
            return Err(format!("block at {} has length {} (minimum 5)", cursor, block_len));
        }
        if cursor + block_len > data.len() {
            return Err(format!(
                "block at {} runs past end of file (length {}, file len {})",
                cursor,
                block_len,
                data.len()
            ));
        }

        let payload = &data[cursor + 5..cursor + block_len];
        match block_id {
            BLOCK_CREATOR => rzx.creator = Some(parse_creator(payload)?),
            BLOCK_SNAPSHOT => rzx.snapshot = Some(parse_snapshot(payload)?),
            BLOCK_INPUT => rzx.recordings.push(parse_input(payload)?),
            other => rzx.unknown_blocks.push(UnknownBlock {
                id: other,
                payload: payload.to_vec(),
            }),
        }

        cursor += block_len;
    }

    Ok(rzx)
}

fn parse_creator(payload: &[u8]) -> Result<Creator, String> {
    if payload.len() < 24 {
        return Err(format!("creator block payload {} bytes (need 24)", payload.len()));
    }
    let name_bytes = &payload[..20];
    let name_end = name_bytes.iter().position(|&b| b == 0).unwrap_or(20);
    let name = String::from_utf8_lossy(&name_bytes[..name_end]).into_owned();
    let version_major = u16::from_le_bytes([payload[20], payload[21]]);
    let version_minor = u16::from_le_bytes([payload[22], payload[23]]);
    Ok(Creator { name, version_major, version_minor })
}

fn parse_snapshot(payload: &[u8]) -> Result<Snapshot, String> {
    if payload.len() < 12 {
        return Err(format!("snapshot block payload {} bytes (need 12)", payload.len()));
    }
    let flags = u32::from_le_bytes([payload[0], payload[1], payload[2], payload[3]]);
    let ext_bytes = &payload[4..8];
    let ext_end = ext_bytes.iter().position(|&b| b == 0).unwrap_or(4);
    let extension = String::from_utf8_lossy(&ext_bytes[..ext_end]).into_owned();
    let uncompressed_len = u32::from_le_bytes([payload[8], payload[9], payload[10], payload[11]]) as usize;

    if flags & SNAPSHOT_FLAG_EXTERNAL != 0 {
        return Err("RZX external snapshot references are not supported".into());
    }

    let raw = &payload[12..];
    let data = if flags & SNAPSHOT_FLAG_COMPRESSED != 0 {
        let mut out = Vec::with_capacity(uncompressed_len);
        ZlibDecoder::new(raw)
            .read_to_end(&mut out)
            .map_err(|e| format!("snapshot zlib decode failed: {}", e))?;
        out
    } else {
        raw.to_vec()
    };

    Ok(Snapshot { flags, extension, data })
}

fn parse_input(payload: &[u8]) -> Result<InputRecording, String> {
    if payload.len() < 18 {
        return Err(format!("input block payload {} bytes (need 18)", payload.len()));
    }
    let frame_count = u32::from_le_bytes([payload[0], payload[1], payload[2], payload[3]]);
    // Reserved byte at offset 4 (should be 0). We ignore.
    let tstate_counter = u32::from_le_bytes([payload[5], payload[6], payload[7], payload[8]]);
    let flags = u32::from_le_bytes([payload[9], payload[10], payload[11], payload[12]]);

    // The frame stream lives at offset 13 — but the version-0.13 spec puts
    // the flags before the stream (so frames start at offset 13). Older
    // 0.12 files put the stream slightly differently. We target 0.13.
    let raw = &payload[13..];
    let stream = if flags & INPUT_FLAG_COMPRESSED != 0 {
        let mut out = Vec::new();
        ZlibDecoder::new(raw)
            .read_to_end(&mut out)
            .map_err(|e| format!("input zlib decode failed: {}", e))?;
        out
    } else {
        raw.to_vec()
    };

    let frames = parse_frames(&stream, frame_count as usize)?;
    Ok(InputRecording { flags, tstate_counter, frames })
}

fn parse_frames(stream: &[u8], expected: usize) -> Result<Vec<Frame>, String> {
    let mut frames = Vec::with_capacity(expected);
    let mut cursor = 0usize;
    let mut last_inputs: Vec<u8> = Vec::new();

    while cursor < stream.len() && frames.len() < expected {
        if cursor + 4 > stream.len() {
            return Err(format!("frame {} header truncated", frames.len()));
        }
        let fetch_count = u16::from_le_bytes([stream[cursor], stream[cursor + 1]]);
        let in_count = u16::from_le_bytes([stream[cursor + 2], stream[cursor + 3]]);
        cursor += 4;

        let inputs = if in_count == SAME_AS_PREVIOUS {
            last_inputs.clone()
        } else {
            let n = in_count as usize;
            if cursor + n > stream.len() {
                return Err(format!(
                    "frame {} input bytes truncated (need {} from offset {}, have {})",
                    frames.len(),
                    n,
                    cursor,
                    stream.len() - cursor
                ));
            }
            let bytes = stream[cursor..cursor + n].to_vec();
            cursor += n;
            last_inputs = bytes.clone();
            bytes
        };

        frames.push(Frame { fetch_count, inputs });
    }

    if frames.len() != expected {
        return Err(format!(
            "RZX header claims {} frames but stream had {}",
            expected,
            frames.len()
        ));
    }
    Ok(frames)
}

/// Serialise an `Rzx` back to bytes. Round-trips structurally for any
/// `Rzx` value the parser produced.
pub fn write(rzx: &Rzx) -> Result<Vec<u8>, String> {
    let mut out = Vec::new();
    out.extend_from_slice(MAGIC);
    out.push(rzx.version_major);
    out.push(rzx.version_minor);
    out.extend_from_slice(&rzx.flags.to_le_bytes());

    if let Some(c) = &rzx.creator {
        write_creator(&mut out, c);
    }
    if let Some(s) = &rzx.snapshot {
        write_snapshot(&mut out, s)?;
    }
    for r in &rzx.recordings {
        write_input(&mut out, r)?;
    }
    for u in &rzx.unknown_blocks {
        write_unknown(&mut out, u);
    }

    Ok(out)
}

fn write_block_header(out: &mut Vec<u8>, id: u8, payload_len: usize) -> usize {
    let total = 5 + payload_len;
    out.push(id);
    let len_offset = out.len();
    out.extend_from_slice(&(total as u32).to_le_bytes());
    len_offset
}

fn write_creator(out: &mut Vec<u8>, c: &Creator) {
    write_block_header(out, BLOCK_CREATOR, 24);
    let mut name_bytes = [0u8; 20];
    let n = c.name.len().min(20);
    name_bytes[..n].copy_from_slice(&c.name.as_bytes()[..n]);
    out.extend_from_slice(&name_bytes);
    out.extend_from_slice(&c.version_major.to_le_bytes());
    out.extend_from_slice(&c.version_minor.to_le_bytes());
}

fn write_snapshot(out: &mut Vec<u8>, s: &Snapshot) -> Result<(), String> {
    if s.flags & SNAPSHOT_FLAG_EXTERNAL != 0 {
        return Err("external snapshot references are not supported on write".into());
    }
    let snapshot_bytes = if s.flags & SNAPSHOT_FLAG_COMPRESSED != 0 {
        let mut encoder = ZlibEncoder::new(Vec::new(), Compression::default());
        encoder
            .write_all(&s.data)
            .map_err(|e| format!("snapshot zlib encode failed: {}", e))?;
        encoder.finish().map_err(|e| format!("snapshot zlib finish failed: {}", e))?
    } else {
        s.data.clone()
    };

    let payload_len = 12 + snapshot_bytes.len();
    write_block_header(out, BLOCK_SNAPSHOT, payload_len);
    out.extend_from_slice(&s.flags.to_le_bytes());
    let mut ext_bytes = [0u8; 4];
    let n = s.extension.len().min(4);
    ext_bytes[..n].copy_from_slice(&s.extension.as_bytes()[..n]);
    out.extend_from_slice(&ext_bytes);
    out.extend_from_slice(&(s.data.len() as u32).to_le_bytes());
    out.extend_from_slice(&snapshot_bytes);
    Ok(())
}

fn write_input(out: &mut Vec<u8>, r: &InputRecording) -> Result<(), String> {
    let mut stream = Vec::new();
    let mut last_inputs: &[u8] = &[];
    for f in &r.frames {
        stream.extend_from_slice(&f.fetch_count.to_le_bytes());
        if f.inputs == last_inputs {
            stream.extend_from_slice(&SAME_AS_PREVIOUS.to_le_bytes());
        } else {
            stream.extend_from_slice(&(f.inputs.len() as u16).to_le_bytes());
            stream.extend_from_slice(&f.inputs);
            last_inputs = &f.inputs;
        }
    }

    let stream_bytes = if r.flags & INPUT_FLAG_COMPRESSED != 0 {
        let mut encoder = ZlibEncoder::new(Vec::new(), Compression::default());
        encoder
            .write_all(&stream)
            .map_err(|e| format!("input zlib encode failed: {}", e))?;
        encoder.finish().map_err(|e| format!("input zlib finish failed: {}", e))?
    } else {
        stream
    };

    let payload_len = 13 + stream_bytes.len();
    write_block_header(out, BLOCK_INPUT, payload_len);
    out.extend_from_slice(&(r.frames.len() as u32).to_le_bytes());
    out.push(0); // reserved
    out.extend_from_slice(&r.tstate_counter.to_le_bytes());
    out.extend_from_slice(&r.flags.to_le_bytes());
    out.extend_from_slice(&stream_bytes);
    Ok(())
}

fn write_unknown(out: &mut Vec<u8>, u: &UnknownBlock) {
    write_block_header(out, u.id, u.payload.len());
    out.extend_from_slice(&u.payload);
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample_rzx(compress: bool) -> Rzx {
        let snap_flags = if compress { SNAPSHOT_FLAG_COMPRESSED } else { 0 };
        let input_flags = if compress { INPUT_FLAG_COMPRESSED } else { 0 };
        Rzx {
            version_major: 0,
            version_minor: 13,
            flags: 0,
            creator: Some(Creator {
                name: "Emu198x".into(),
                version_major: 0,
                version_minor: 1,
            }),
            snapshot: Some(Snapshot {
                flags: snap_flags,
                extension: "z80".into(),
                data: vec![0xAA, 0xBB, 0xCC, 0xDD, 0xEE, 0xFF, 0x00, 0x11, 0x22, 0x33],
            }),
            recordings: vec![InputRecording {
                flags: input_flags,
                tstate_counter: 0,
                frames: vec![
                    Frame { fetch_count: 1234, inputs: vec![0xFF, 0x7E] },
                    Frame { fetch_count: 1234, inputs: vec![0xFF, 0x7E] }, // → SAME_AS_PREVIOUS
                    Frame { fetch_count: 1235, inputs: vec![0xBF, 0x7E] },
                    Frame { fetch_count: 1234, inputs: vec![] },
                ],
            }],
            unknown_blocks: vec![],
        }
    }

    #[test]
    fn header_round_trip() {
        let rzx = Rzx {
            version_major: 0,
            version_minor: 13,
            flags: 0xCAFEBABE,
            ..Default::default()
        };
        let bytes = write(&rzx).unwrap();
        let parsed = parse(&bytes).unwrap();
        assert_eq!(parsed.version_major, 0);
        assert_eq!(parsed.version_minor, 13);
        assert_eq!(parsed.flags, 0xCAFEBABE);
        assert!(parsed.creator.is_none());
        assert!(parsed.snapshot.is_none());
        assert!(parsed.recordings.is_empty());
    }

    #[test]
    fn rejects_bad_magic() {
        let mut bytes = vec![0u8; 10];
        bytes[..4].copy_from_slice(b"FAKE");
        assert!(parse(&bytes).is_err());
    }

    #[test]
    fn rejects_truncated_header() {
        assert!(parse(b"RZX!").is_err());
    }

    #[test]
    fn round_trip_uncompressed() {
        let original = sample_rzx(false);
        let bytes = write(&original).unwrap();
        let parsed = parse(&bytes).unwrap();

        // Creator
        let c = parsed.creator.as_ref().unwrap();
        assert_eq!(c.name, "Emu198x");
        assert_eq!(c.version_major, 0);
        assert_eq!(c.version_minor, 1);

        // Snapshot
        let s = parsed.snapshot.as_ref().unwrap();
        assert_eq!(s.flags, 0);
        assert_eq!(s.extension, "z80");
        assert_eq!(s.data, original.snapshot.as_ref().unwrap().data);

        // Frames
        let r = &parsed.recordings[0];
        assert_eq!(r.frames.len(), 4);
        assert_eq!(r.frames, original.recordings[0].frames);
    }

    #[test]
    fn round_trip_compressed() {
        let original = sample_rzx(true);
        let bytes = write(&original).unwrap();
        let parsed = parse(&bytes).unwrap();

        // Decoded snapshot data should match the original despite the
        // compressed wire format.
        let s = parsed.snapshot.as_ref().unwrap();
        assert_eq!(s.flags & SNAPSHOT_FLAG_COMPRESSED, SNAPSHOT_FLAG_COMPRESSED);
        assert_eq!(s.data, original.snapshot.as_ref().unwrap().data);

        let r = &parsed.recordings[0];
        assert_eq!(r.flags & INPUT_FLAG_COMPRESSED, INPUT_FLAG_COMPRESSED);
        assert_eq!(r.frames, original.recordings[0].frames);
    }

    #[test]
    fn same_as_previous_marker_round_trips() {
        let original = sample_rzx(false);
        let bytes = write(&original).unwrap();

        // Locate the input block payload manually and confirm the second
        // frame uses the 0xFFFF marker on disk. The frame stream starts
        // at: header (10) + creator block (5+24=29) + snapshot block
        // (5 + 12 + 10 = 27) + input block header (5) + input block
        // pre-stream (4 frame_count + 1 reserved + 4 tstates + 4 flags = 13)
        let stream_start = 10 + 29 + 27 + 5 + 13;
        // Frame 1: 2 bytes fetch_count + 2 bytes in_count + 2 bytes inputs.
        // Frame 2 starts at +6, fetch_count is 2 bytes, so its in_count is at +8.
        let frame_2_in_count_offset = stream_start + 8;
        let in_count = u16::from_le_bytes([
            bytes[frame_2_in_count_offset],
            bytes[frame_2_in_count_offset + 1],
        ]);
        assert_eq!(
            in_count, SAME_AS_PREVIOUS,
            "second frame must be encoded as SAME_AS_PREVIOUS on disk"
        );

        // And the parser must expand it back to the full inputs.
        let parsed = parse(&bytes).unwrap();
        assert_eq!(parsed.recordings[0].frames[1].inputs, vec![0xFF, 0x7E]);
    }

    #[test]
    fn unknown_block_preserved() {
        // Build an RZX with a custom block ID 0x42 between the header and
        // a creator block, and verify it survives a round trip.
        let mut bytes = Vec::new();
        bytes.extend_from_slice(MAGIC);
        bytes.push(0); // major
        bytes.push(13); // minor
        bytes.extend_from_slice(&0u32.to_le_bytes()); // flags

        // Custom block: 5-byte header + 4-byte payload = 9 bytes total
        bytes.push(0x42);
        bytes.extend_from_slice(&9u32.to_le_bytes());
        bytes.extend_from_slice(&[0xDE, 0xAD, 0xBE, 0xEF]);

        let parsed = parse(&bytes).unwrap();
        assert_eq!(parsed.unknown_blocks.len(), 1);
        assert_eq!(parsed.unknown_blocks[0].id, 0x42);
        assert_eq!(parsed.unknown_blocks[0].payload, vec![0xDE, 0xAD, 0xBE, 0xEF]);

        // Re-emit and re-parse — the unknown block should still be there.
        let written = write(&parsed).unwrap();
        let reparsed = parse(&written).unwrap();
        assert_eq!(reparsed.unknown_blocks.len(), 1);
        assert_eq!(reparsed.unknown_blocks[0].id, 0x42);
        assert_eq!(reparsed.unknown_blocks[0].payload, vec![0xDE, 0xAD, 0xBE, 0xEF]);
    }
}
