//! Parser for the Commodore 1541 **G64** raw-GCR disk-image format.
//!
//! Unlike a `D64` (a decoded sector image), a `G64` stores the raw GCR
//! bitstream of each half-track exactly as it was written — custom sync marks,
//! non-standard sector counts, fat/half-tracks, extra tracks and density
//! tricks all survive. That is what copy-protected originals rely on, so a
//! faithful drive reads a G64 where the D64 layer cannot represent the disk.
//!
//! # Layout (little-endian)
//!
//! | Offset | Field |
//! |--------|-------|
//! | 0..8   | signature `GCR-1541` (the double-sided `GCR-1571`/G71 is rejected) |
//! | 8      | version (0) |
//! | 9      | number of half-tracks (typically 84) |
//! | 10..12 | maximum track length in bytes (`u16`) |
//! | 12..   | track-offset table: one `u32` per half-track (0 = no data) |
//! | …      | speed-zone table: one `u32` per half-track |
//! | @offset| per track: `u16` length, then that many GCR bytes |
//!
//! The half-track table index `i` (0-based) is the drive's own half-track slot:
//! it maps to head position `i + 2`, i.e. track `i / 2 + 1` (even `i` = whole
//! track, odd `i` = the half-track above it). This is the same indexing the
//! 1541 core uses, so a parsed track drops straight into its track slots.
//!
//! v1 is read-only and models each track's speed as a constant zone; a speed
//! table entry that points at a per-byte speed block (value > 3) falls back to
//! the standard zone for that track.

use thiserror::Error;

/// The G64 (single-sided 1541) signature.
pub const G64_SIGNATURE: &[u8; 8] = b"GCR-1541";
/// The G71 (double-sided 1571) signature — detected but not supported in v1.
pub const G71_SIGNATURE: &[u8; 8] = b"GCR-1571";

/// The version byte this parser understands.
const SUPPORTED_VERSION: u8 = 0;
/// A G64 never exceeds 84 half-tracks (tracks 1–42 in half steps) in practice;
/// this bounds the tables so a corrupt count cannot allocate wildly.
const MAX_HALF_TRACKS: usize = 84;
/// Header size: signature + version + count + max-track-length.
const HEADER_LEN: usize = 12;

/// One raw-GCR half-track from a G64 image.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct G64Track {
    /// The raw GCR bytes as written, exactly the length the drive head sees per
    /// revolution (it wraps at `gcr.len()`).
    pub gcr: Vec<u8>,
    /// The track's constant speed zone (0–3). Approximated to the standard zone
    /// for the physical track when the image uses a per-byte speed block.
    pub speed_zone: u8,
}

/// A parsed G64 image: raw GCR per half-track slot.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct G64Image {
    /// Image version byte.
    pub version: u8,
    /// The image's declared maximum track length.
    pub max_track_length: u16,
    /// One entry per half-track slot (index = head position − 2), `None` for an
    /// unformatted or absent half-track.
    pub half_tracks: Vec<Option<G64Track>>,
}

/// Errors from [`parse`].
#[derive(Clone, Debug, Error, PartialEq, Eq)]
pub enum G64ParseError {
    /// The image is shorter than the header or a declared table/track.
    #[error("G64 image is truncated: needed {needed} bytes at offset {offset}, image is {len}")]
    Truncated {
        offset: usize,
        needed: usize,
        len: usize,
    },
    /// The signature is neither `GCR-1541` nor `GCR-1571`.
    #[error("not a G64 image: bad signature")]
    BadSignature,
    /// A `GCR-1571` (double-sided G71) image, which v1 does not support.
    #[error("G71 (double-sided GCR-1571) images are not supported yet")]
    G71NotSupported,
    /// The version byte is not understood.
    #[error("unsupported G64 version {0}")]
    UnsupportedVersion(u8),
    /// The half-track count is zero or exceeds the sane maximum.
    #[error("unsupported G64 half-track count {0}")]
    BadHalfTrackCount(u8),
    /// A track offset points outside the image.
    #[error("track slot {slot} offset {offset} is out of range (image {len})")]
    TrackOffsetOutOfRange {
        slot: usize,
        offset: usize,
        len: usize,
    },
    /// A track's declared length is zero or exceeds the image's maximum.
    #[error("track slot {slot} length {length} is invalid (max {max})")]
    BadTrackLength { slot: usize, length: u16, max: u16 },
}

/// Parses a G64 image into per-half-track raw GCR.
///
/// # Errors
///
/// Returns a [`G64ParseError`] when the signature, version, counts, or any
/// track offset/length are invalid, or the image is truncated.
pub fn parse(bytes: &[u8]) -> Result<G64Image, G64ParseError> {
    if bytes.len() < HEADER_LEN {
        return Err(G64ParseError::Truncated {
            offset: 0,
            needed: HEADER_LEN,
            len: bytes.len(),
        });
    }

    let signature = &bytes[0..8];
    if signature == G71_SIGNATURE {
        return Err(G64ParseError::G71NotSupported);
    }
    if signature != G64_SIGNATURE {
        return Err(G64ParseError::BadSignature);
    }

    let version = bytes[8];
    if version != SUPPORTED_VERSION {
        return Err(G64ParseError::UnsupportedVersion(version));
    }

    let num_half_tracks = bytes[9];
    if num_half_tracks == 0 || usize::from(num_half_tracks) > MAX_HALF_TRACKS {
        return Err(G64ParseError::BadHalfTrackCount(num_half_tracks));
    }
    let num_half_tracks = usize::from(num_half_tracks);
    let max_track_length = read_u16(bytes, 10);

    // The offset table follows the header; the speed table follows the offset
    // table. Each is `num_half_tracks` × u32.
    let offset_table = HEADER_LEN;
    let speed_table = offset_table + num_half_tracks * 4;
    let tables_end = speed_table + num_half_tracks * 4;
    if bytes.len() < tables_end {
        return Err(G64ParseError::Truncated {
            offset: offset_table,
            needed: tables_end,
            len: bytes.len(),
        });
    }

    let mut half_tracks = Vec::with_capacity(num_half_tracks);
    for slot in 0..num_half_tracks {
        let track_offset = read_u32(bytes, offset_table + slot * 4) as usize;
        if track_offset == 0 {
            half_tracks.push(None);
            continue;
        }
        // A non-zero offset must leave room for the 2-byte length prefix.
        if track_offset + 2 > bytes.len() {
            return Err(G64ParseError::TrackOffsetOutOfRange {
                slot,
                offset: track_offset,
                len: bytes.len(),
            });
        }
        let track_len = read_u16(bytes, track_offset);
        if track_len == 0 || track_len > max_track_length {
            return Err(G64ParseError::BadTrackLength {
                slot,
                length: track_len,
                max: max_track_length,
            });
        }
        let data_start = track_offset + 2;
        let data_end = data_start + usize::from(track_len);
        if data_end > bytes.len() {
            return Err(G64ParseError::Truncated {
                offset: data_start,
                needed: data_end,
                len: bytes.len(),
            });
        }

        let speed_raw = read_u32(bytes, speed_table + slot * 4);
        let speed_zone = if speed_raw <= 3 {
            speed_raw as u8
        } else {
            // A per-byte speed block (offset > 3): approximate with the standard
            // zone for this track. Rare among protected originals.
            standard_speed_zone(slot)
        };

        half_tracks.push(Some(G64Track {
            gcr: bytes[data_start..data_end].to_vec(),
            speed_zone,
        }));
    }

    Ok(G64Image {
        version,
        max_track_length,
        half_tracks,
    })
}

/// The standard C64 speed zone for the physical track at half-track slot
/// `slot` (slot 0 = track 1). Tracks 1–17 → 3, 18–24 → 2, 25–30 → 1, 31+ → 0.
#[must_use]
pub const fn standard_speed_zone(slot: usize) -> u8 {
    let track = slot / 2 + 1;
    match track {
        1..=17 => 3,
        18..=24 => 2,
        25..=30 => 1,
        _ => 0,
    }
}

fn read_u16(bytes: &[u8], at: usize) -> u16 {
    u16::from_le_bytes([bytes[at], bytes[at + 1]])
}

fn read_u32(bytes: &[u8], at: usize) -> u32 {
    u32::from_le_bytes([bytes[at], bytes[at + 1], bytes[at + 2], bytes[at + 3]])
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Builds a minimal well-formed G64 with the given per-slot tracks (each an
    /// optional `(gcr, speed)`), packing the offset/speed tables and track data.
    fn build_g64(tracks: &[Option<(Vec<u8>, u8)>], max_len: u16) -> Vec<u8> {
        let n = tracks.len();
        let mut buf = Vec::new();
        buf.extend_from_slice(G64_SIGNATURE);
        buf.push(0); // version
        buf.push(n as u8);
        buf.extend_from_slice(&max_len.to_le_bytes());

        // Track data lands after both tables; compute its base.
        let data_base = HEADER_LEN + n * 4 + n * 4;
        let mut data = Vec::new();
        let mut offsets = vec![0u32; n];
        let mut speeds = vec![0u32; n];
        for (slot, track) in tracks.iter().enumerate() {
            if let Some((gcr, speed)) = track {
                offsets[slot] = (data_base + data.len()) as u32;
                speeds[slot] = u32::from(*speed);
                data.extend_from_slice(&(gcr.len() as u16).to_le_bytes());
                data.extend_from_slice(gcr);
            }
        }
        for off in &offsets {
            buf.extend_from_slice(&off.to_le_bytes());
        }
        for sp in &speeds {
            buf.extend_from_slice(&sp.to_le_bytes());
        }
        buf.extend_from_slice(&data);
        buf
    }

    #[test]
    fn parses_a_minimal_two_track_image() {
        let img = build_g64(
            &[
                Some((vec![0xFF, 0x52, 0x54], 3)),
                None,
                Some((vec![0xAA; 10], 2)),
            ],
            7928,
        );
        let parsed = parse(&img).expect("valid G64");
        assert_eq!(parsed.version, 0);
        assert_eq!(parsed.max_track_length, 7928);
        assert_eq!(parsed.half_tracks.len(), 3);
        assert_eq!(
            parsed.half_tracks[0],
            Some(G64Track {
                gcr: vec![0xFF, 0x52, 0x54],
                speed_zone: 3,
            })
        );
        assert_eq!(parsed.half_tracks[1], None);
        assert_eq!(
            parsed.half_tracks[2]
                .as_ref()
                .expect("track present")
                .gcr
                .len(),
            10
        );
        assert_eq!(
            parsed.half_tracks[2]
                .as_ref()
                .expect("track present")
                .speed_zone,
            2
        );
    }

    #[test]
    fn rejects_bad_signature_and_g71() {
        let mut bad = build_g64(&[Some((vec![0x55], 3))], 7928);
        bad[0] = b'X';
        assert_eq!(parse(&bad), Err(G64ParseError::BadSignature));

        let mut g71 = build_g64(&[Some((vec![0x55], 3))], 7928);
        g71[0..8].copy_from_slice(G71_SIGNATURE);
        assert_eq!(parse(&g71), Err(G64ParseError::G71NotSupported));
    }

    #[test]
    fn rejects_bad_version_and_count() {
        let mut ver = build_g64(&[Some((vec![0x55], 3))], 7928);
        ver[8] = 1;
        assert_eq!(parse(&ver), Err(G64ParseError::UnsupportedVersion(1)));

        let mut count = build_g64(&[Some((vec![0x55], 3))], 7928);
        count[9] = 0;
        assert_eq!(parse(&count), Err(G64ParseError::BadHalfTrackCount(0)));
    }

    #[test]
    fn rejects_a_track_length_over_the_maximum() {
        // max_track_length 4, but the track data is 8 bytes.
        let img = build_g64(&[Some((vec![0x55; 8], 3))], 4);
        assert!(matches!(
            parse(&img),
            Err(G64ParseError::BadTrackLength { slot: 0, .. })
        ));
    }

    #[test]
    fn per_byte_speed_block_falls_back_to_the_standard_zone() {
        // Speed value > 3 (a file offset) → standard zone for the track.
        let img = build_g64(&[Some((vec![0x55; 4], 200))], 7928);
        let parsed = parse(&img).expect("valid G64");
        assert_eq!(
            parsed.half_tracks[0]
                .as_ref()
                .expect("track present")
                .speed_zone,
            3
        );
    }

    #[test]
    fn standard_zone_boundaries() {
        assert_eq!(standard_speed_zone(0), 3); // track 1
        assert_eq!(standard_speed_zone(34), 2); // track 18 (slot 34)
        assert_eq!(standard_speed_zone(48), 1); // track 25 (slot 48)
        assert_eq!(standard_speed_zone(60), 0); // track 31 (slot 60)
    }
}
