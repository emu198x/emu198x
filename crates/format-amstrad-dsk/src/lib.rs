//! Compatibility adapter from the shared CPCEMU DSK codec to FDC-owned state.
//! The decoder and its history graduated to Format198x; controller types stay
//! here so existing machine state serialisation and call sites remain stable.

use nec_upd765a::{DiskImage, DiskSector, DiskTrack};

/// Decode standard/extended DSK and preserve every sector's address and status.
///
/// # Errors
/// Returns the shared decoder's validation error for malformed containers.
pub fn parse(data: &[u8]) -> Result<DiskImage, String> {
    let decoded = format198x_amstrad_cpc_dsk::parse(data)?;
    Ok(DiskImage {
        sides: decoded.sides,
        tracks_per_side: decoded.tracks_per_side,
        tracks: decoded
            .tracks
            .into_iter()
            .map(|side| {
                side.into_iter()
                    .map(|track| DiskTrack {
                        sectors: track
                            .sectors
                            .into_iter()
                            .map(|s| DiskSector {
                                c: s.c,
                                h: s.h,
                                id: s.id,
                                n: s.n,
                                st1: s.st1,
                                st2: s.st2,
                                data: s.data,
                            })
                            .collect(),
                    })
                    .collect()
            })
            .collect(),
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    const HEADER_LEN: usize = 256;
    const TRACK_HEADER_LEN: usize = 256;
    const SECTOR_INFO_LEN: usize = 8;
    const SIG_STANDARD: &[u8] = b"MV - CPC";
    const SIG_EXTENDED: &[u8] = b"EXTENDED CPC DSK File";

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
        let image = parse(&data).expect("the DSK fixture parses");
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
        let image = parse(&data).expect("the DSK fixture parses");
        let s = image.sector(0, 0, 5).expect("sector 5 is present");
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

        let image = parse(&buf).expect("the DSK fixture parses");
        let track = &image.tracks[0][0];
        // Physical order matches the SIL.
        assert_eq!(track.sectors[0].id, 0xC3);
        assert_eq!(track.sectors[1].id, 0xC1);
        assert_eq!(track.sectors[2].id, 0xC2);
        // Lookup by ID hits the right sector.
        assert_eq!(
            image
                .sector(0, 0, 0xC1)
                .expect("sector 0xC1 is present")
                .data[0],
            0xC1
        );
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
        assert_eq!(
            track.sectors[1].data.len(),
            0,
            "zero-length sector keeps an empty data Vec"
        );
        assert_eq!(track.sectors[1].st1 & 0x20, 0x20, "ST1.DE carried through");
        assert_eq!(track.sectors[1].st2 & 0x20, 0x20, "ST2.DD carried through");
    }
}
