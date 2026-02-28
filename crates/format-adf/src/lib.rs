//! Amiga Disk File (ADF) image parser.
//!
//! ADF is a raw sector dump: 80 cylinders x 2 heads x 11 sectors x 512 bytes
//! = 901,120 bytes for double-density disks. HD disks double the sector count.

use std::fmt;

pub const SECTOR_SIZE: u32 = 512;
pub const SECTORS_PER_TRACK_DD: u32 = 11;
pub const SECTORS_PER_TRACK_HD: u32 = 22;
pub const CYLINDERS: u32 = 80;
pub const HEADS: u32 = 2;
pub const ADF_SIZE_DD: usize = (CYLINDERS * HEADS * SECTORS_PER_TRACK_DD * SECTOR_SIZE) as usize;
pub const ADF_SIZE_HD: usize = (CYLINDERS * HEADS * SECTORS_PER_TRACK_HD * SECTOR_SIZE) as usize;

#[derive(Debug)]
pub enum AdfError {
    InvalidSize(usize),
}

impl fmt::Display for AdfError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::InvalidSize(size) => write!(
                f,
                "invalid ADF size: {} bytes (expected {} for DD or {} for HD)",
                size, ADF_SIZE_DD, ADF_SIZE_HD,
            ),
        }
    }
}

impl std::error::Error for AdfError {}

pub struct Adf {
    data: Vec<u8>,
    sectors_per_track: u32,
}

impl Adf {
    pub fn from_bytes(data: Vec<u8>) -> Result<Self, AdfError> {
        let sectors_per_track = match data.len() {
            ADF_SIZE_DD => SECTORS_PER_TRACK_DD,
            ADF_SIZE_HD => SECTORS_PER_TRACK_HD,
            other => return Err(AdfError::InvalidSize(other)),
        };
        Ok(Self {
            data,
            sectors_per_track,
        })
    }

    pub fn sectors_per_track(&self) -> u32 {
        self.sectors_per_track
    }

    fn offset(&self, cyl: u32, head: u32, sector: u32) -> usize {
        ((cyl * HEADS + head) * self.sectors_per_track + sector) as usize * SECTOR_SIZE as usize
    }

    pub fn read_sector(&self, cyl: u32, head: u32, sector: u32) -> &[u8] {
        let start = self.offset(cyl, head, sector);
        &self.data[start..start + SECTOR_SIZE as usize]
    }

    pub fn write_sector(&mut self, cyl: u32, head: u32, sector: u32, data: &[u8]) {
        let start = self.offset(cyl, head, sector);
        self.data[start..start + SECTOR_SIZE as usize].copy_from_slice(data);
    }

    /// Return the raw ADF image data.
    pub fn data(&self) -> &[u8] {
        &self.data
    }

    pub fn read_track_sectors(&self, cyl: u32, head: u32) -> &[u8] {
        let start = self.offset(cyl, head, 0);
        let len = self.sectors_per_track as usize * SECTOR_SIZE as usize;
        &self.data[start..start + len]
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reject_invalid_size() {
        assert!(Adf::from_bytes(vec![0; 100]).is_err());
    }

    #[test]
    fn accept_dd_size() {
        let adf = Adf::from_bytes(vec![0; ADF_SIZE_DD]);
        assert!(adf.is_ok());
        assert_eq!(
            adf.expect("valid").sectors_per_track(),
            SECTORS_PER_TRACK_DD
        );
    }

    #[test]
    fn accept_hd_size() {
        let adf = Adf::from_bytes(vec![0; ADF_SIZE_HD]);
        assert!(adf.is_ok());
        assert_eq!(
            adf.expect("valid").sectors_per_track(),
            SECTORS_PER_TRACK_HD
        );
    }

    #[test]
    fn read_write_round_trip() {
        let mut adf = Adf::from_bytes(vec![0; ADF_SIZE_DD]).expect("valid");
        let data: Vec<u8> = (0..SECTOR_SIZE).map(|i| (i & 0xFF) as u8).collect();
        adf.write_sector(40, 1, 5, &data);
        assert_eq!(adf.read_sector(40, 1, 5), &data[..]);
    }

    #[test]
    fn offset_correctness() {
        let adf = Adf::from_bytes(vec![0; ADF_SIZE_DD]).expect("valid");
        // Track 0 = cyl 0, head 0 -> offset 0
        assert_eq!(adf.offset(0, 0, 0), 0);
        // Track 1 = cyl 0, head 1 -> offset 11*512
        assert_eq!(adf.offset(0, 1, 0), 11 * 512);
        // Track 2 = cyl 1, head 0 -> offset 22*512
        assert_eq!(adf.offset(1, 0, 0), 22 * 512);
        // Sector 3 of track 2
        assert_eq!(adf.offset(1, 0, 3), (22 + 3) * 512);
    }

    #[test]
    fn read_track_sectors_length() {
        let adf = Adf::from_bytes(vec![0; ADF_SIZE_DD]).expect("valid");
        let track = adf.read_track_sectors(10, 0);
        assert_eq!(track.len(), 11 * 512);
    }
}
