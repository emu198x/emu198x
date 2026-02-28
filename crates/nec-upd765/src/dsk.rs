//! DSK and Extended DSK (EDSK) disk image parser.
//!
//! Parses the standard CPC-emu disk image format used by +3 software.
//! Standard DSK has fixed track sizes; extended DSK has per-track sizes.
//!
//! # Format
//!
//! Standard header: `"MV - CPCEMU Disk-File\r\nDisk-Info\r\n"`
//! Extended header: `"EXTENDED CPC DSK File\r\nDisk-Info\r\n"`

#![allow(clippy::cast_possible_truncation)]

/// A parsed DSK disk image.
pub struct DskImage {
    pub tracks: Vec<DskTrack>,
    pub sides: u8,
    /// Whether this was an extended DSK (affects serialisation).
    extended: bool,
}

/// A single track on the disk.
pub struct DskTrack {
    pub track_num: u8,
    pub side: u8,
    pub sectors: Vec<DskSector>,
}

/// A single sector within a track.
pub struct DskSector {
    /// Cylinder (C) from the sector ID field.
    pub c: u8,
    /// Head (H) from the sector ID field.
    pub h: u8,
    /// Sector ID (R).
    pub r: u8,
    /// Size code (N). Actual size = 128 << N.
    pub n: u8,
    /// FDC status register 1 (for copy protection).
    pub st1: u8,
    /// FDC status register 2 (for copy protection).
    pub st2: u8,
    /// Sector data.
    pub data: Vec<u8>,
}

const STANDARD_HEADER: &[u8] = b"MV - CPCEMU Disk-File\r\nDisk-Info\r\n";
const EXTENDED_HEADER: &[u8] = b"EXTENDED CPC DSK File\r\nDisk-Info\r\n";

/// Parse a DSK or EDSK file from raw bytes.
///
/// # Errors
///
/// Returns an error string if the data is too short, has an invalid
/// header, or contains malformed track/sector data.
pub fn parse_dsk(data: &[u8]) -> Result<DskImage, String> {
    if data.len() < 256 {
        return Err("DSK file too short for header".to_string());
    }

    let extended = if data[..EXTENDED_HEADER.len()] == *EXTENDED_HEADER {
        true
    } else if data[..STANDARD_HEADER.len()] == *STANDARD_HEADER {
        false
    } else {
        return Err("Not a valid DSK file (unrecognised header)".to_string());
    };

    let num_tracks = data[0x30] as usize;
    let num_sides = data[0x31];

    if extended {
        parse_extended(data, num_tracks, num_sides)
    } else {
        let track_size = u16::from_le_bytes([data[0x32], data[0x33]]) as usize;
        parse_standard(data, num_tracks, num_sides, track_size)
    }
}

fn parse_standard(
    data: &[u8],
    num_tracks: usize,
    num_sides: u8,
    track_size: usize,
) -> Result<DskImage, String> {
    let mut tracks = Vec::new();
    let total = num_tracks * num_sides as usize;

    for i in 0..total {
        let offset = 0x100 + i * track_size;
        if offset + 0x100 > data.len() {
            break; // Truncated image — parse what we can
        }
        let track = parse_track_info(&data[offset..], false)?;
        tracks.push(track);
    }

    Ok(DskImage {
        tracks,
        sides: num_sides,
        extended: false,
    })
}

fn parse_extended(
    data: &[u8],
    num_tracks: usize,
    num_sides: u8,
) -> Result<DskImage, String> {
    let mut tracks = Vec::new();
    let total = num_tracks * num_sides as usize;

    // Track size table starts at offset $34, one byte per track (in units of 256 bytes)
    let mut offset = 0x100;
    for i in 0..total {
        let size_entry = if 0x34 + i < data.len() {
            data[0x34 + i] as usize * 256
        } else {
            0
        };

        if size_entry == 0 {
            // Unformatted track — skip
            continue;
        }

        if offset + 0x100 > data.len() {
            break;
        }

        let track = parse_track_info(&data[offset..], true)?;
        tracks.push(track);
        offset += size_entry;
    }

    Ok(DskImage {
        tracks,
        sides: num_sides,
        extended: true,
    })
}

fn parse_track_info(data: &[u8], extended: bool) -> Result<DskTrack, String> {
    // Track-Info block: 0x00-0x0C = "Track-Info\r\n"
    if data.len() < 0x18 {
        return Err("Track info block too short".to_string());
    }

    let track_num = data[0x10];
    let side = data[0x11];
    let sector_size_code = data[0x14]; // Default N for standard DSK
    let num_sectors = data[0x15] as usize;

    let mut sectors = Vec::with_capacity(num_sectors);
    let mut sector_data_offset = 0x100; // Sector data starts after the 256-byte track info

    for s in 0..num_sectors {
        let info_offset = 0x18 + s * 8;
        if info_offset + 8 > data.len() {
            break;
        }

        let c = data[info_offset];
        let h = data[info_offset + 1];
        let r = data[info_offset + 2];
        let n = data[info_offset + 3];
        let st1 = data[info_offset + 4];
        let st2 = data[info_offset + 5];

        let actual_size = if extended {
            // EDSK: per-sector actual data length stored in info bytes 6-7
            u16::from_le_bytes([data[info_offset + 6], data[info_offset + 7]]) as usize
        } else {
            // Standard: all sectors use the track's sector size code
            128usize << (sector_size_code as u32)
        };

        let sector_data = if sector_data_offset + actual_size <= data.len() {
            data[sector_data_offset..sector_data_offset + actual_size].to_vec()
        } else {
            // Pad with zeros if data is truncated
            let mut d = vec![0u8; actual_size];
            let avail = data.len().saturating_sub(sector_data_offset);
            if avail > 0 {
                d[..avail].copy_from_slice(&data[sector_data_offset..sector_data_offset + avail]);
            }
            d
        };

        sector_data_offset += actual_size;

        sectors.push(DskSector {
            c,
            h,
            r,
            n,
            st1,
            st2,
            data: sector_data,
        });
    }

    Ok(DskTrack {
        track_num,
        side,
        sectors,
    })
}

impl DskImage {
    /// Read a sector by track number, side, and sector ID (R value).
    #[must_use]
    pub fn read_sector(&self, track: u8, side: u8, sector_id: u8) -> Option<&[u8]> {
        let trk = self.tracks.iter().find(|t| t.track_num == track && t.side == side)?;
        let sec = trk.sectors.iter().find(|s| s.r == sector_id)?;
        Some(&sec.data)
    }

    /// Write data to a sector by track number, side, and sector ID.
    /// Returns true if the sector was found and written.
    pub fn write_sector(&mut self, track: u8, side: u8, sector_id: u8, data: &[u8]) -> bool {
        let Some(trk) = self.tracks.iter_mut().find(|t| t.track_num == track && t.side == side)
        else {
            return false;
        };
        let Some(sec) = trk.sectors.iter_mut().find(|s| s.r == sector_id) else {
            return false;
        };
        let len = data.len().min(sec.data.len());
        sec.data[..len].copy_from_slice(&data[..len]);
        true
    }

    /// Get the sector IDs (C, H, R, N) for all sectors on a given track/side.
    #[must_use]
    pub fn track_ids(&self, track: u8, side: u8) -> Vec<(u8, u8, u8, u8)> {
        self.tracks
            .iter()
            .find(|t| t.track_num == track && t.side == side)
            .map(|t| t.sectors.iter().map(|s| (s.c, s.h, s.r, s.n)).collect())
            .unwrap_or_default()
    }

    /// Serialise back to DSK format bytes.
    #[must_use]
    pub fn to_bytes(&self) -> Vec<u8> {
        let mut out = Vec::new();

        // Disk information block (256 bytes)
        let mut header = [0u8; 256];
        let sig = if self.extended { EXTENDED_HEADER } else { STANDARD_HEADER };
        header[..sig.len()].copy_from_slice(sig);

        // Count tracks per side
        let max_track = self
            .tracks
            .iter()
            .map(|t| t.track_num)
            .max()
            .unwrap_or(0) as usize
            + 1;
        header[0x30] = max_track as u8;
        header[0x31] = self.sides;

        if self.extended {
            // Track size table at $34
            let total = max_track * self.sides as usize;

            // Build track data first to calculate sizes
            let mut track_blocks: Vec<Option<Vec<u8>>> = vec![None; total];
            for trk in &self.tracks {
                let idx = trk.track_num as usize * self.sides as usize + trk.side as usize;
                if idx < total {
                    track_blocks[idx] = Some(serialise_track(trk));
                }
            }

            for (i, block) in track_blocks.iter().enumerate() {
                if let Some(b) = block {
                    header[0x34 + i] = (b.len() / 256) as u8;
                }
            }

            out.extend_from_slice(&header);
            for block in track_blocks.into_iter().flatten() {
                out.extend_from_slice(&block);
            }
        } else {
            // Standard: uniform track size
            let track_data: Vec<Vec<u8>> = self.tracks.iter().map(serialise_track).collect();
            let max_size = track_data.iter().map(Vec::len).max().unwrap_or(0);
            let track_size = ((max_size + 255) / 256) * 256;
            header[0x32] = (track_size & 0xFF) as u8;
            header[0x33] = ((track_size >> 8) & 0xFF) as u8;

            out.extend_from_slice(&header);
            for td in &track_data {
                out.extend_from_slice(td);
                // Pad to uniform track size
                if td.len() < track_size {
                    out.resize(out.len() + track_size - td.len(), 0);
                }
            }
        }

        out
    }
}

fn serialise_track(trk: &DskTrack) -> Vec<u8> {
    let mut buf = vec![0u8; 256]; // Track info block

    // "Track-Info\r\n"
    buf[..12].copy_from_slice(b"Track-Info\r\n");
    buf[0x10] = trk.track_num;
    buf[0x11] = trk.side;
    // Sector size code from first sector (or 2 = 512 bytes default)
    buf[0x14] = trk.sectors.first().map_or(2, |s| s.n);
    buf[0x15] = trk.sectors.len() as u8;
    buf[0x16] = 0x4E; // GAP#3 length (standard)
    buf[0x17] = 0xE5; // Filler byte

    for (i, sec) in trk.sectors.iter().enumerate() {
        let off = 0x18 + i * 8;
        if off + 8 > 256 {
            break;
        }
        buf[off] = sec.c;
        buf[off + 1] = sec.h;
        buf[off + 2] = sec.r;
        buf[off + 3] = sec.n;
        buf[off + 4] = sec.st1;
        buf[off + 5] = sec.st2;
        let size = sec.data.len() as u16;
        buf[off + 6] = (size & 0xFF) as u8;
        buf[off + 7] = ((size >> 8) & 0xFF) as u8;
    }

    // Append sector data
    for sec in &trk.sectors {
        buf.extend_from_slice(&sec.data);
    }

    // Pad to 256-byte boundary
    let padded = ((buf.len() + 255) / 256) * 256;
    buf.resize(padded, 0);

    buf
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Build a minimal standard DSK with 1 track, 1 side, 1 sector.
    fn make_standard_dsk() -> Vec<u8> {
        let mut data = vec![0u8; 0x100]; // Disk info block

        // Header
        data[..STANDARD_HEADER.len()].copy_from_slice(STANDARD_HEADER);
        data[0x30] = 1; // 1 track
        data[0x31] = 1; // 1 side

        // Track size = 256 (info) + 512 (one sector) = 768 = 0x0300
        data[0x32] = 0x00;
        data[0x33] = 0x03;

        // Track info block (256 bytes)
        let mut track = vec![0u8; 256];
        track[..12].copy_from_slice(b"Track-Info\r\n");
        track[0x10] = 0;    // Track 0
        track[0x11] = 0;    // Side 0
        track[0x14] = 2;    // Sector size code 2 = 512 bytes
        track[0x15] = 1;    // 1 sector
        // Sector info at 0x18
        track[0x18] = 0;    // C
        track[0x19] = 0;    // H
        track[0x1A] = 0x01; // R (sector ID 1)
        track[0x1B] = 2;    // N
        data.extend_from_slice(&track);

        // Sector data: 512 bytes
        let mut sector_data = vec![0xE5u8; 512];
        sector_data[0] = 0xAA;
        sector_data[511] = 0xBB;
        data.extend_from_slice(&sector_data);

        data
    }

    #[test]
    fn parse_standard_dsk_header() {
        let raw = make_standard_dsk();
        let img = parse_dsk(&raw).expect("should parse");
        assert_eq!(img.sides, 1);
        assert_eq!(img.tracks.len(), 1);
        assert_eq!(img.tracks[0].track_num, 0);
        assert_eq!(img.tracks[0].side, 0);
        assert_eq!(img.tracks[0].sectors.len(), 1);
    }

    #[test]
    fn parse_sector_data() {
        let raw = make_standard_dsk();
        let img = parse_dsk(&raw).expect("should parse");
        let sec = &img.tracks[0].sectors[0];
        assert_eq!(sec.r, 0x01);
        assert_eq!(sec.n, 2);
        assert_eq!(sec.data.len(), 512);
        assert_eq!(sec.data[0], 0xAA);
        assert_eq!(sec.data[511], 0xBB);
    }

    #[test]
    fn read_sector_by_id() {
        let raw = make_standard_dsk();
        let img = parse_dsk(&raw).expect("should parse");
        let data = img.read_sector(0, 0, 1).expect("sector 1 exists");
        assert_eq!(data[0], 0xAA);
    }

    #[test]
    fn read_missing_sector_returns_none() {
        let raw = make_standard_dsk();
        let img = parse_dsk(&raw).expect("should parse");
        assert!(img.read_sector(0, 0, 99).is_none());
        assert!(img.read_sector(1, 0, 1).is_none());
    }

    #[test]
    fn write_sector_roundtrip() {
        let raw = make_standard_dsk();
        let mut img = parse_dsk(&raw).expect("should parse");

        let new_data = vec![0x42u8; 512];
        assert!(img.write_sector(0, 0, 1, &new_data));

        let read_back = img.read_sector(0, 0, 1).expect("sector exists");
        assert_eq!(read_back[0], 0x42);
        assert_eq!(read_back[511], 0x42);
    }

    #[test]
    fn track_ids() {
        let raw = make_standard_dsk();
        let img = parse_dsk(&raw).expect("should parse");
        let ids = img.track_ids(0, 0);
        assert_eq!(ids, vec![(0, 0, 1, 2)]);
    }

    #[test]
    fn serialise_roundtrip() {
        let raw = make_standard_dsk();
        let img = parse_dsk(&raw).expect("should parse");
        let serialised = img.to_bytes();
        let img2 = parse_dsk(&serialised).expect("re-parse should work");
        assert_eq!(img2.tracks.len(), 1);
        assert_eq!(img2.tracks[0].sectors[0].data[0], 0xAA);
    }

    #[test]
    fn extended_dsk_parse() {
        // Build a minimal EDSK with one track, one sector of 256 bytes
        let mut data = vec![0u8; 0x100];
        data[..EXTENDED_HEADER.len()].copy_from_slice(EXTENDED_HEADER);
        data[0x30] = 1; // 1 track
        data[0x31] = 1; // 1 side
        // Track size table at $34: track 0 = (256 info + 256 data) / 256 = 2
        data[0x34] = 2;

        // Track info
        let mut track = vec![0u8; 256];
        track[..12].copy_from_slice(b"Track-Info\r\n");
        track[0x10] = 0;
        track[0x11] = 0;
        track[0x14] = 1; // N=1 (256 bytes)
        track[0x15] = 1; // 1 sector
        // Sector info
        track[0x18] = 0;    // C
        track[0x19] = 0;    // H
        track[0x1A] = 1;    // R
        track[0x1B] = 1;    // N
        // EDSK per-sector size in bytes 6-7
        track[0x1E] = 0x00; // Low byte: 256
        track[0x1F] = 0x01; // High byte
        data.extend_from_slice(&track);

        // Sector data: 256 bytes
        let mut sec_data = vec![0u8; 256];
        sec_data[0] = 0xCC;
        data.extend_from_slice(&sec_data);

        let img = parse_dsk(&data).expect("should parse EDSK");
        assert_eq!(img.tracks[0].sectors[0].data.len(), 256);
        assert_eq!(img.tracks[0].sectors[0].data[0], 0xCC);
    }

    #[test]
    fn invalid_header_errors() {
        let data = vec![0u8; 256];
        assert!(parse_dsk(&data).is_err());
    }
}
