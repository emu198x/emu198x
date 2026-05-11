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
        return Err(format!(
            "DSK reports {} sides; only 1 or 2 supported",
            sides
        ));
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
        let standard_len = u16::from_le_bytes([data[0x32], data[0x33]]) as usize;
        if standard_len == 0 {
            return Err("Standard DSK header reports zero-length tracks".into());
        }
        track_lengths.resize(entries, standard_len);
    }

    // Walk the tracks. Each present track gets parsed; absent tracks
    // (length 0 in EDSK) become empty placeholders so the indices line up.
    let mut tracks_per_side_vec: Vec<Vec<DiskTrack>> = (0..sides)
        .map(|_| Vec::with_capacity(tracks_per_side as usize))
        .collect();

    let mut cursor = HEADER_LEN;
    for (entry, &length) in track_lengths.iter().enumerate().take(entries) {
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
        let parsed = parse_track(track_block, extended)
            .map_err(|e| format!("Track {} (offset {}): {}", entry, cursor, e))?;
        tracks_per_side_vec[side].push(parsed);
        cursor += length;
    }

    Ok(DiskImage {
        sides,
        tracks_per_side,
        tracks: tracks_per_side_vec,
    })
}

fn parse_track(block: &[u8], extended: bool) -> Result<DiskTrack, String> {
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

    // Parse the SIL into (C, H, R, N, ST1, ST2, length, source) tuples.
    // The full address-mark CHRN is needed for `ReadId` and header
    // verification — copy-protected disks deliberately record
    // mismatched values. The recorded ST1 / ST2 carry the FDC status
    // the chip would have returned reading this sector at dump time:
    // ST1.DE (data CRC error), ST2.CM (deleted-mark = DDAM), and the
    // related error bits are how Speedlock / Alkatraz / Spectra hide
    // their key sectors from a naïve sector-by-sector copier.
    //
    // EDSK records the actual (possibly per-sector) data length in
    // bytes 6..8 of each entry. **Crucially, an EDSK length of 0
    // means "this sector has no data bytes in the image"** — the
    // dumper couldn't reliably read it, so it noted the address-mark
    // CHRN + the FDC status bits but stored no data. We must NOT
    // fall back to N for those: Tetris's track 12 is a format-only
    // protection track with sectors whose N rises to 0x07 (claimed
    // 16384 bytes) but whose stored data is zero. Falling back to
    // 128<<N would advance the data cursor 16384 bytes past the
    // end of the track block and trip the bounds check.
    //
    // Standard DSK (non-extended) is rare for protected titles and
    // does use the N fallback because there's no per-sector length
    // field. We honour that: when the EDSK signature ISN'T present
    // we treat zero `edsk_len` as "use track default".
    let mut sectors_info = Vec::with_capacity(sector_count);
    for i in 0..sector_count {
        let off = 0x18 + i * SECTOR_INFO_LEN;
        let c = block[off];
        let h = block[off + 1];
        let id = block[off + 2];
        let n = block[off + 3];
        let st1 = block[off + 4];
        let st2 = block[off + 5];
        let edsk_len = u16::from_le_bytes([block[off + 6], block[off + 7]]) as usize;
        let len = if extended {
            // EDSK: zero is meaningful — sector has no data bytes
            // stored. The chip would still find the address mark
            // (the SIL entry exists), so the loader can probe CHRN,
            // but a read-data attempt either fails or returns
            // garbage. We model that with an empty data Vec.
            edsk_len
        } else if edsk_len != 0 {
            edsk_len
        } else {
            track_default_size
        };
        sectors_info.push((c, h, id, n, st1, st2, len));
    }

    // Sector data follows the track header (256 bytes), packed in the
    // order listed in the SIL.
    let mut data_cursor = TRACK_HEADER_LEN;
    let mut sectors = Vec::with_capacity(sector_count);
    for (c, h, id, n, st1, st2, len) in sectors_info {
        if data_cursor + len > block.len() {
            return Err(format!(
                "sector ID {:#04x} runs past track block (need {} bytes at offset {})",
                id, len, data_cursor
            ));
        }
        sectors.push(DiskSector {
            c,
            h,
            id,
            n,
            st1,
            st2,
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
            buf[off] = 0; // C
            buf[off + 1] = 0; // H
            buf[off + 2] = (i + 1) as u8; // R
            buf[off + 3] = 2; // N
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

    /// EDSK protection tracks (Tetris's track 12, for instance) list
    /// sectors whose SIL `data length` is zero — the dumper saw the
    /// sector's address mark but couldn't or wouldn't capture any
    /// data bytes. The parser must treat that as "0 bytes of data
    /// stored", not "fall back to 128 << N" (which can be 8 KiB+ on
    /// these protection layouts and runs past the track block end).
    #[test]
    fn edsk_zero_length_sector_is_zero_bytes_not_n_fallback() {
        const TRACK_HEADER_LEN: usize = 256;
        const HEADER_LEN: usize = 256;
        const SECTOR_INFO_LEN: usize = 8;

        // Two sectors: one normal 512-byte sector, one "address-mark
        // only" sector with N=7 (claimed 16 KiB) but stored length 0.
        // If the parser fell back to 128 << 7 (=16384) it would
        // try to copy 16 KiB starting after the first sector's 512
        // bytes and run off the end of the track block.
        let track_total = TRACK_HEADER_LEN + 512; // one real sector
        let mut buf = vec![0u8; HEADER_LEN + track_total];
        buf[..SIG_EXTENDED.len()].copy_from_slice(SIG_EXTENDED);
        buf[0x30] = 1;
        buf[0x31] = 1;
        buf[0x34] = (track_total / 256) as u8;

        let t = HEADER_LEN;
        buf[t..t + b"Track-Info\r\n".len()].copy_from_slice(b"Track-Info\r\n");
        buf[t + 0x14] = 2;
        buf[t + 0x15] = 2; // two sectors

        // Sector 1: normal, R=1, N=2 (512 bytes), stored len = 512.
        let off1 = t + 0x18;
        buf[off1 + 2] = 1;
        buf[off1 + 3] = 2;
        let len1 = 512u16.to_le_bytes();
        buf[off1 + 6] = len1[0];
        buf[off1 + 7] = len1[1];

        // Sector 2: protection. R=2, N=7 (claims 16 KiB), stored len = 0.
        // ST1.DE + ST2.DD set to mark the bad CRC the dumper saw.
        let off2 = t + 0x18 + SECTOR_INFO_LEN;
        buf[off2 + 2] = 2;
        buf[off2 + 3] = 7;
        buf[off2 + 4] = 0x20; // ST1.DE
        buf[off2 + 5] = 0x20; // ST2.DD
        // bytes 6..8 stay zero — that's the load-bearing bit

        // Fill the one real sector's data.
        for (i, b) in buf
            .iter_mut()
            .enumerate()
            .skip(t + TRACK_HEADER_LEN)
            .take(512)
        {
            *b = (i & 0xff) as u8;
        }

        let image = parse(&buf).expect("zero-length sector should parse cleanly");
        let track = &image.tracks[0][0];
        assert_eq!(track.sectors.len(), 2);
        assert_eq!(track.sectors[0].id, 1);
        assert_eq!(track.sectors[0].data.len(), 512);
        assert_eq!(track.sectors[1].id, 2);
        assert_eq!(track.sectors[1].data.len(), 0, "zero-length sector keeps an empty data Vec");
        assert_eq!(track.sectors[1].st1 & 0x20, 0x20, "ST1.DE carried through");
        assert_eq!(track.sectors[1].st2 & 0x20, 0x20, "ST2.DD carried through");
    }
}
