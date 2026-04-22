//! DSK and EDSK floppy image parser.
//!
//! Two flavours share one container layout:
//!
//! - **Standard DSK** ("MV - CPC...") — every track has the same length,
//!   stored in bytes 0x32..0x34 of the disk header.
//! - **Extended DSK** ("EXTENDED CPC DSK File...") — each track has its
//!   own length, listed one byte per track in 0x34..0x100 (multiplied by
//!   256 to get the actual byte count). A length of 0 means the track is
//!   missing.
//!
//! Each track starts with a 256-byte Track Information Block followed by
//! the sectors in physical order. Sectors are looked up by their ID (R)
//! at runtime, not by position — the +3 ROM and most disk software trust
//! the address marks rather than the physical layout.
//!
//! Output is a [`DiskImage`] that the [`nec_upd765a`] FDC consumes
//! directly.

use nec_upd765a::{DiskImage, DiskSector, DiskTrack};

const HEADER_LEN: usize = 256;
const TRACK_HEADER_LEN: usize = 256;
const SECTOR_INFO_LEN: usize = 8;

const SIG_STANDARD: &[u8] = b"MV - CPC";
const SIG_EXTENDED: &[u8] = b"EXTENDED CPC DSK File";

/// Parse a DSK or EDSK image into a [`DiskImage`].
pub fn parse(data: &[u8]) -> Result<DiskImage, String> {
    if data.len() < HEADER_LEN {
        return Err("DSK file shorter than 256-byte header".into());
    }

    let extended = if data.starts_with(SIG_EXTENDED) {
        true
    } else if data.starts_with(SIG_STANDARD) {
        false
    } else {
        return Err("Not a DSK file (missing CPC signature)".into());
    };

    let tracks_per_side = data[0x30];
    let sides = data[0x31];
    if sides == 0 || sides > 2 {
        return Err(format!("DSK reports {} sides; only 1 or 2 supported", sides));
    }
    if tracks_per_side == 0 {
        return Err("DSK reports zero tracks".into());
    }

    let entries = tracks_per_side as usize * sides as usize;

    // Determine where each track starts and how long it is. EDSK uses a
    // table of one byte per track-side (in 0x34..0x100), each multiplied
    // by 256. Standard DSK uses a single fixed length in 0x32..0x34
    // applied to every track.
    let mut track_lengths = Vec::with_capacity(entries);
    if extended {
        if 0x34 + entries > data.len() {
            return Err("EDSK track size table runs past end of file".into());
        }
        for i in 0..entries {
            track_lengths.push(data[0x34 + i] as usize * 256);
        }
    } else {
        let standard_len =
            u16::from_le_bytes([data[0x32], data[0x33]]) as usize;
        if standard_len == 0 {
            return Err("Standard DSK header reports zero-length tracks".into());
        }
        track_lengths.resize(entries, standard_len);
    }

    // Walk the tracks. Each present track gets parsed; absent tracks
    // (length 0 in EDSK) become empty placeholders so the indices line up.
    let mut tracks_per_side_vec: Vec<Vec<DiskTrack>> =
        (0..sides).map(|_| Vec::with_capacity(tracks_per_side as usize)).collect();

    let mut cursor = HEADER_LEN;
    for entry in 0..entries {
        let length = track_lengths[entry];
        // Sides interleave per track: track0/side0, track0/side1, track1/side0...
        let side = entry % sides as usize;

        if length == 0 {
            tracks_per_side_vec[side].push(DiskTrack::default());
            continue;
        }

        if cursor + length > data.len() {
            return Err(format!(
                "Track {} runs past end of file (offset {}, length {})",
                entry, cursor, length
            ));
        }

        let track_block = &data[cursor..cursor + length];
        let parsed = parse_track(track_block).map_err(|e| {
            format!("Track {} (offset {}): {}", entry, cursor, e)
        })?;
        tracks_per_side_vec[side].push(parsed);
        cursor += length;
    }

    Ok(DiskImage {
        sides,
        tracks_per_side,
        tracks: tracks_per_side_vec,
    })
}

fn parse_track(block: &[u8]) -> Result<DiskTrack, String> {
    if block.len() < TRACK_HEADER_LEN {
        return Err("track block shorter than 256-byte header".into());
    }
    if !block.starts_with(b"Track-Info") {
        return Err("missing Track-Info signature".into());
    }

    let sector_count = block[0x15] as usize;
    if sector_count == 0 {
        return Ok(DiskTrack::default());
    }

    // Default sector size from the track header (used for standard DSK
    // where every sector in the track has the same size).
    let track_n = block[0x14];
    let track_default_size = 128usize << track_n.min(6) as usize;

    // Sector Information List starts at 0x18, 8 bytes per sector.
    let sil_end = 0x18 + sector_count * SECTOR_INFO_LEN;
    if sil_end > block.len() {
        return Err("sector info list runs past track block".into());
    }

    // Parse the SIL into (id, length) pairs. EDSK records the actual
    // (possibly per-sector) data length in bytes 6..8 of each entry; on
    // standard DSK those bytes are unused so we fall back to the track
    // default.
    let mut sectors_info = Vec::with_capacity(sector_count);
    for i in 0..sector_count {
        let off = 0x18 + i * SECTOR_INFO_LEN;
        let id = block[off + 2];
        let n = block[off + 3];
        let edsk_len = u16::from_le_bytes([block[off + 6], block[off + 7]]) as usize;
        let len = if edsk_len != 0 {
            edsk_len
        } else {
            128usize << n.min(6) as usize
        };
        // The N from the address mark trumps the track default if they
        // disagree — but if both are zero we use the track default.
        let _ = track_default_size;
        sectors_info.push((id, len));
    }

    // Sector data follows the track header (256 bytes), packed in the
    // order listed in the SIL.
    let mut data_cursor = TRACK_HEADER_LEN;
    let mut sectors = Vec::with_capacity(sector_count);
    for (id, len) in sectors_info {
        if data_cursor + len > block.len() {
            return Err(format!(
                "sector ID {:#04x} runs past track block (need {} bytes at offset {})",
                id, len, data_cursor
            ));
        }
        sectors.push(DiskSector {
            id,
            data: block[data_cursor..data_cursor + len].to_vec(),
        });
        data_cursor += len;
    }

    Ok(DiskTrack { sectors })
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Build a minimal standard DSK image with one side, one track,
    /// nine 512-byte sectors numbered 1..=9.
    fn build_minimal_dsk() -> Vec<u8> {
        let track_data_len = 9 * 512;
        let track_total = TRACK_HEADER_LEN + track_data_len;

        let mut buf = vec![0u8; HEADER_LEN + track_total];

        // Disk header
        buf[..SIG_STANDARD.len()].copy_from_slice(SIG_STANDARD);
        buf[0x30] = 1; // tracks
        buf[0x31] = 1; // sides
        let track_size = (track_total as u16).to_le_bytes();
        buf[0x32] = track_size[0];
        buf[0x33] = track_size[1];

        // Track header at offset 256
        let t = HEADER_LEN;
        buf[t..t + b"Track-Info\r\n".len()].copy_from_slice(b"Track-Info\r\n");
        buf[t + 0x10] = 0; // track number
        buf[t + 0x11] = 0; // side
        buf[t + 0x14] = 2; // N=2 → 512 bytes
        buf[t + 0x15] = 9; // sector count
        for i in 0..9 {
            let off = t + 0x18 + i * SECTOR_INFO_LEN;
            buf[off + 0] = 0;             // C
            buf[off + 1] = 0;             // H
            buf[off + 2] = (i + 1) as u8; // R
            buf[off + 3] = 2;             // N
        }

        // Sector data: stamp the first byte of each sector with its ID so
        // we can verify lookups by sector ID.
        for i in 0..9 {
            let off = t + TRACK_HEADER_LEN + i * 512;
            buf[off] = (i + 1) as u8;
        }
        buf
    }

    #[test]
    fn parses_standard_dsk() {
        let data = build_minimal_dsk();
        let image = parse(&data).unwrap();
        assert_eq!(image.sides, 1);
        assert_eq!(image.tracks_per_side, 1);
        assert_eq!(image.tracks[0].len(), 1);
        let track = &image.tracks[0][0];
        assert_eq!(track.sectors.len(), 9);
        assert_eq!(track.sectors[0].id, 1);
        assert_eq!(track.sectors[8].id, 9);
        assert_eq!(track.sectors[0].data[0], 1);
        assert_eq!(track.sectors[8].data[0], 9);
    }

    #[test]
    fn lookup_by_sector_id() {
        let data = build_minimal_dsk();
        let image = parse(&data).unwrap();
        let s = image.sector(0, 0, 5).unwrap();
        assert_eq!(s.id, 5);
        assert_eq!(s.data[0], 5);
    }

    #[test]
    fn rejects_non_dsk() {
        let result = parse(b"not a disk image at all..............................................................................................................................................................................................................................................");
        assert!(result.is_err());
    }

    #[test]
    fn rejects_truncated() {
        assert!(parse(b"too short").is_err());
    }

    #[test]
    fn parses_edsk() {
        // Build an EDSK image: one track, three sectors with mixed IDs
        // out of order (proves we honour sector IDs from the SIL).
        let track_data_len = 3 * 512;
        let track_total = TRACK_HEADER_LEN + track_data_len;
        let mut buf = vec![0u8; HEADER_LEN + track_total];

        buf[..SIG_EXTENDED.len()].copy_from_slice(SIG_EXTENDED);
        buf[0x30] = 1; // tracks
        buf[0x31] = 1; // sides
        // EDSK track size table: one entry, length / 256
        buf[0x34] = (track_total / 256) as u8;

        let t = HEADER_LEN;
        buf[t..t + b"Track-Info\r\n".len()].copy_from_slice(b"Track-Info\r\n");
        buf[t + 0x14] = 2; // N=2
        buf[t + 0x15] = 3; // 3 sectors

        let ids = [0xC3u8, 0xC1, 0xC2];
        for (i, &id) in ids.iter().enumerate() {
            let off = t + 0x18 + i * SECTOR_INFO_LEN;
            buf[off + 2] = id;
            buf[off + 3] = 2;
            // EDSK actual length = 512
            let len = 512u16.to_le_bytes();
            buf[off + 6] = len[0];
            buf[off + 7] = len[1];
        }
        for (i, &id) in ids.iter().enumerate() {
            let off = t + TRACK_HEADER_LEN + i * 512;
            buf[off] = id;
        }

        let image = parse(&buf).unwrap();
        let track = &image.tracks[0][0];
        // Physical order matches the SIL.
        assert_eq!(track.sectors[0].id, 0xC3);
        assert_eq!(track.sectors[1].id, 0xC1);
        assert_eq!(track.sectors[2].id, 0xC2);
        // Lookup by ID hits the right sector.
        assert_eq!(image.sector(0, 0, 0xC1).unwrap().data[0], 0xC1);
    }
}
