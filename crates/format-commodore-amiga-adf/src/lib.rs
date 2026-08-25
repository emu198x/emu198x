//! Amiga Disk File (ADF) image parser.
//!
//! ADF is a raw sector dump: 80 cylinders x 2 heads x 11 sectors x 512 bytes
//! = 901,120 bytes for double-density disks. HD disks double the sector count.
//!
//! Ported from `~/Projects/Emu198x-archive/crates/format-commodore-amiga-adf/src/lib.rs`.

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
    /// The file is a disk image in a container this reader does not
    /// handle. Named rather than sized, because a size complaint about a
    /// format the file never was sends the reader to check their disk
    /// image when the answer is that we cannot read this kind (#1192).
    NotAnAdf {
        /// Short name of the format the magic identifies.
        format: &'static str,
        /// What it is, in a clause that finishes "…, which this reader
        /// does not handle".
        detail: &'static str,
    },
}

/// Identify a container by its leading bytes.
///
/// Deliberately small: these are the formats an Amiga disk actually
/// arrives in, and naming one wrongly would be worse than not naming it.
/// Anything unrecognised falls through to the size check, which is the
/// right answer for a truncated or padded ADF.
#[must_use]
pub fn identify_container(data: &[u8]) -> Option<(&'static str, &'static str)> {
    const CANDIDATES: &[(&[u8], &str, &str)] = &[
        (
            b"CAPS",
            "IPF",
            "a flux-level image from the Software Preservation Society",
        ),
        (
            b"UAE-1ADF",
            "extended ADF",
            "UAE's variable-length-track ADF",
        ),
        (b"DMS!", "DMS", "a Disk Masher System archive"),
        (b"PK\x03\x04", "zip", "a zip archive — extract it first"),
        (b"\x1f\x8b", "gzip", "a gzip stream, most likely an .adz"),
    ];
    CANDIDATES
        .iter()
        .find(|(magic, _, _)| data.starts_with(magic))
        .map(|(_, format, detail)| (*format, *detail))
}

impl fmt::Display for AdfError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::InvalidSize(size) => write!(
                f,
                "invalid ADF size: {} bytes (expected {} for DD or {} for HD)",
                size, ADF_SIZE_DD, ADF_SIZE_HD,
            ),
            Self::NotAnAdf { format, detail } => write!(
                f,
                "this is not an ADF: the file is {format} — {detail}, which this reader does not handle"
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
        // Ask what the file is before complaining about how big it is.
        // TOSEC files the Amiga's commercial releases under Games/SPS in
        // IPF, so for anyone working from a TOSEC set the clean dumps are
        // exactly the ones that land here.
        if let Some((format, detail)) = identify_container(&data) {
            return Err(AdfError::NotAnAdf { format, detail });
        }
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
    #![allow(clippy::unwrap_used)]

    use super::*;

    #[test]
    fn reject_invalid_size() {
        assert!(Adf::from_bytes(vec![0; 100]).is_err());
    }

    #[test]
    fn accept_dd_size() {
        let adf = Adf::from_bytes(vec![0; ADF_SIZE_DD]);
        assert!(adf.is_ok());
        assert_eq!(adf.unwrap().sectors_per_track(), SECTORS_PER_TRACK_DD);
    }

    #[test]
    fn accept_hd_size() {
        let adf = Adf::from_bytes(vec![0; ADF_SIZE_HD]);
        assert!(adf.is_ok());
        assert_eq!(adf.unwrap().sectors_per_track(), SECTORS_PER_TRACK_HD);
    }

    #[test]
    fn read_write_round_trip() {
        let mut adf = Adf::from_bytes(vec![0; ADF_SIZE_DD]).unwrap();
        let data: Vec<u8> = (0..SECTOR_SIZE).map(|i| (i & 0xFF) as u8).collect();
        adf.write_sector(40, 1, 5, &data);
        assert_eq!(adf.read_sector(40, 1, 5), &data[..]);
    }

    #[test]
    fn offset_correctness() {
        let adf = Adf::from_bytes(vec![0; ADF_SIZE_DD]).unwrap();
        assert_eq!(adf.offset(0, 0, 0), 0);
        assert_eq!(adf.offset(0, 1, 0), 11 * 512);
        assert_eq!(adf.offset(1, 0, 0), 22 * 512);
        assert_eq!(adf.offset(1, 0, 3), (22 + 3) * 512);
    }

    #[test]
    fn read_track_sectors_length() {
        let adf = Adf::from_bytes(vec![0; ADF_SIZE_DD]).unwrap();
        let track = adf.read_track_sectors(10, 0);
        assert_eq!(track.len(), 11 * 512);
    }
}

#[cfg(test)]
mod container_tests {
    use super::*;

    /// `expect_err` would need `Debug` on `Adf`, and a derived one would
    /// print the whole disk image.
    fn err_of(result: Result<Adf, AdfError>) -> AdfError {
        match result {
            Err(err) => err,
            Ok(_) => panic!("expected a rejection, got a disk"),
        }
    }

    /// The reported case: an IPF was rejected as a wrong-sized ADF,
    /// which sent the reader to check a disk image that was never at
    /// fault. Magic verified against a real SPS dump.
    #[test]
    fn an_ipf_is_named_rather_than_measured() {
        let mut ipf = b"CAPS".to_vec();
        ipf.extend_from_slice(&[0, 0, 0, 0x0C, 0x1C, 0xD5, 0x73, 0xBA]);
        let err = err_of(Adf::from_bytes(ipf));
        let message = err.to_string();
        assert!(message.contains("IPF"), "{message}");
        assert!(
            !message.contains("invalid ADF size"),
            "the size is not what is wrong: {message}"
        );
    }

    #[test]
    fn other_containers_are_named_too() {
        for (bytes, expected) in [
            (b"UAE-1ADF".to_vec(), "extended ADF"),
            (b"DMS!".to_vec(), "DMS"),
            (b"PK\x03\x04".to_vec(), "zip"),
            (vec![0x1f, 0x8b, 0x08, 0x00], "gzip"),
        ] {
            let err = err_of(Adf::from_bytes(bytes));
            assert!(err.to_string().contains(expected), "{}", err);
        }
    }

    /// A truncated or padded ADF is still an ADF, and the size complaint
    /// is the right answer for it.
    #[test]
    fn an_unrecognised_file_still_gets_the_size_complaint() {
        let err = err_of(Adf::from_bytes(vec![0u8; 1024]));
        assert!(err.to_string().contains("invalid ADF size"), "{}", err);
    }

    #[test]
    fn a_real_adf_still_loads() {
        assert!(Adf::from_bytes(vec![0u8; ADF_SIZE_DD]).is_ok());
        assert!(Adf::from_bytes(vec![0u8; ADF_SIZE_HD]).is_ok());
    }
}
