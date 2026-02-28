//! D64 disk image parser.
//!
//! A D64 image contains 35 tracks with variable sectors per track:
//!   Tracks  1-17: 21 sectors (zone 0)
//!   Tracks 18-24: 19 sectors (zone 1)
//!   Tracks 25-30: 18 sectors (zone 2)
//!   Tracks 31-35: 17 sectors (zone 3)
//!
//! Total: 683 sectors x 256 bytes = 174,848 bytes.
//! D64 images may also be 175,531 bytes (with per-sector error info).

/// Standard D64 size: 683 sectors x 256 bytes.
const D64_SIZE: usize = 174_848;
/// D64 with error info: 683 sectors + 683 error bytes.
const D64_SIZE_WITH_ERRORS: usize = 175_531;
/// Bytes per sector.
const SECTOR_SIZE: usize = 256;

/// Sectors per track, indexed by track number (1-based, so index 0 is unused).
const SECTORS_PER_TRACK: [u8; 36] = [
    0, // track 0 doesn't exist
    21, 21, 21, 21, 21, 21, 21, 21, 21, 21, 21, 21, 21, 21, 21, 21, 21, // 1-17
    19, 19, 19, 19, 19, 19, 19, // 18-24
    18, 18, 18, 18, 18, 18, // 25-30
    17, 17, 17, 17, 17, // 31-35
];

/// Byte offset of the first sector of each track (1-indexed).
/// Pre-computed for fast lookup.
const TRACK_OFFSETS: [usize; 36] = {
    let mut offsets = [0usize; 36];
    let mut track = 1;
    let mut offset = 0;
    while track < 36 {
        offsets[track] = offset;
        offset += SECTORS_PER_TRACK[track] as usize * SECTOR_SIZE;
        track += 1;
    }
    offsets
};

/// A parsed D64 disk image.
pub struct D64 {
    data: Vec<u8>,
}

impl D64 {
    /// Parse a D64 image from raw bytes.
    ///
    /// Accepts 174,848 bytes (standard) or 175,531 bytes (with error info).
    pub fn from_bytes(data: &[u8]) -> Result<Self, String> {
        if data.len() != D64_SIZE && data.len() != D64_SIZE_WITH_ERRORS {
            return Err(format!(
                "Invalid D64 size: {} bytes (expected {} or {})",
                data.len(),
                D64_SIZE,
                D64_SIZE_WITH_ERRORS
            ));
        }
        Ok(Self {
            data: data.to_vec(),
        })
    }

    /// Number of sectors on a given track (1-35).
    ///
    /// Returns 0 for invalid track numbers.
    #[must_use]
    pub fn sectors_per_track(track: u8) -> u8 {
        if (1..=35).contains(&track) {
            SECTORS_PER_TRACK[track as usize]
        } else {
            0
        }
    }

    /// Byte offset of a given sector within the image.
    ///
    /// Returns `None` for invalid track/sector numbers.
    #[must_use]
    pub fn sector_offset(track: u8, sector: u8) -> Option<usize> {
        if !(1..=35).contains(&track) {
            return None;
        }
        if sector >= SECTORS_PER_TRACK[track as usize] {
            return None;
        }
        Some(TRACK_OFFSETS[track as usize] + sector as usize * SECTOR_SIZE)
    }

    /// Read a 256-byte sector.
    ///
    /// Returns a reference to the sector data, or `None` for invalid track/sector.
    #[must_use]
    pub fn read_sector(&self, track: u8, sector: u8) -> Option<&[u8]> {
        let offset = Self::sector_offset(track, sector)?;
        Some(&self.data[offset..offset + SECTOR_SIZE])
    }

    /// Write 256 bytes to a sector.
    ///
    /// Returns `false` for invalid track/sector.
    pub fn write_sector(&mut self, track: u8, sector: u8, data: &[u8]) -> bool {
        if data.len() != SECTOR_SIZE {
            return false;
        }
        let Some(offset) = Self::sector_offset(track, sector) else {
            return false;
        };
        self.data[offset..offset + SECTOR_SIZE].copy_from_slice(data);
        true
    }

    /// Get the disk ID from the BAM (track 18, sector 0, bytes $A2-$A3).
    #[must_use]
    pub fn disk_id(&self) -> [u8; 2] {
        let bam = self
            .read_sector(18, 0)
            .expect("track 18 sector 0 always valid");
        [bam[0xA2], bam[0xA3]]
    }

    /// Raw image data.
    #[must_use]
    pub fn data(&self) -> &[u8] {
        &self.data
    }

    /// Return a copy of the D64 image as a byte vector (for saving).
    #[must_use]
    pub fn to_bytes(&self) -> Option<Vec<u8>> {
        Some(self.data.clone())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn make_d64() -> Vec<u8> {
        vec![0; D64_SIZE]
    }

    #[test]
    fn reject_bad_size() {
        assert!(D64::from_bytes(&[0; 100]).is_err());
        assert!(D64::from_bytes(&[0; D64_SIZE + 1]).is_err());
    }

    #[test]
    fn accept_standard_size() {
        assert!(D64::from_bytes(&make_d64()).is_ok());
    }

    #[test]
    fn accept_error_info_size() {
        assert!(D64::from_bytes(&vec![0; D64_SIZE_WITH_ERRORS]).is_ok());
    }

    #[test]
    fn sectors_per_track_values() {
        assert_eq!(D64::sectors_per_track(1), 21);
        assert_eq!(D64::sectors_per_track(17), 21);
        assert_eq!(D64::sectors_per_track(18), 19);
        assert_eq!(D64::sectors_per_track(24), 19);
        assert_eq!(D64::sectors_per_track(25), 18);
        assert_eq!(D64::sectors_per_track(30), 18);
        assert_eq!(D64::sectors_per_track(31), 17);
        assert_eq!(D64::sectors_per_track(35), 17);
        assert_eq!(D64::sectors_per_track(0), 0);
        assert_eq!(D64::sectors_per_track(36), 0);
    }

    #[test]
    fn sector_offset_track1() {
        assert_eq!(D64::sector_offset(1, 0), Some(0));
        assert_eq!(D64::sector_offset(1, 1), Some(256));
        assert_eq!(D64::sector_offset(1, 20), Some(20 * 256));
        assert_eq!(D64::sector_offset(1, 21), None); // Invalid sector
    }

    #[test]
    fn sector_offset_track18() {
        // Track 18 starts after 17 tracks of 21 sectors each
        let expected = 17 * 21 * 256;
        assert_eq!(D64::sector_offset(18, 0), Some(expected));
    }

    #[test]
    fn total_offsets_consistent() {
        // Verify the last sector offset + 256 = D64_SIZE
        let last_offset = D64::sector_offset(35, 16).expect("valid");
        assert_eq!(last_offset + SECTOR_SIZE, D64_SIZE);
    }

    #[test]
    fn sector_round_trip() {
        let mut d64 = D64::from_bytes(&make_d64()).expect("valid");
        let mut test_data = [0u8; 256];
        test_data[0] = 0xAB;
        test_data[255] = 0xCD;
        assert!(d64.write_sector(18, 0, &test_data));
        let read = d64.read_sector(18, 0).expect("valid");
        assert_eq!(read[0], 0xAB);
        assert_eq!(read[255], 0xCD);
    }

    #[test]
    fn disk_id_from_bam() {
        let mut raw = make_d64();
        let bam_offset = D64::sector_offset(18, 0).expect("valid");
        raw[bam_offset + 0xA2] = 0x41; // 'A'
        raw[bam_offset + 0xA3] = 0x42; // 'B'
        let d64 = D64::from_bytes(&raw).expect("valid");
        assert_eq!(d64.disk_id(), [0x41, 0x42]);
    }
}
