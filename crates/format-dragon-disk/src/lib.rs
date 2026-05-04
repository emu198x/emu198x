//! DragonDOS floppy disk image parsing.
//!
//! The common Dragon archive format is VDK: a small `dk` header followed by
//! raw 256-byte sector data in track order. The sample corpus used here stores
//! 40-track, single-sided, 18-sector DragonDOS disks.

use thiserror::Error;

const VDK_SIGNATURE: &[u8; 2] = b"dk";
const VDK_HEADER_LEN: usize = 12;
const DEFAULT_TRACKS: u8 = 40;
const DEFAULT_SIDES: u8 = 1;
const DEFAULT_SECTORS_PER_TRACK: u8 = 18;
const DEFAULT_SECTOR_SIZE: u16 = 256;

/// Parsed DragonDOS disk image.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct DragonDiskImage {
    /// Tracks per side.
    pub tracks: u8,
    /// Number of sides.
    pub sides: u8,
    /// Sectors per track.
    pub sectors_per_track: u8,
    /// Bytes per sector.
    pub sector_size: u16,
    data: Box<[u8]>,
}

impl DragonDiskImage {
    /// Returns a sector by track, side, and one-based sector number.
    #[must_use]
    pub fn sector(&self, track: u8, side: u8, sector: u8) -> Option<&[u8]> {
        let offset = self.sector_offset(track, side, sector)?;
        let sector_size = usize::from(self.sector_size);
        self.data.get(offset..offset + sector_size)
    }

    /// Returns a mutable sector by track, side, and one-based sector number.
    #[must_use]
    pub fn sector_mut(&mut self, track: u8, side: u8, sector: u8) -> Option<&mut [u8]> {
        let offset = self.sector_offset(track, side, sector)?;
        let sector_size = usize::from(self.sector_size);
        self.data.get_mut(offset..offset + sector_size)
    }

    fn sector_offset(&self, track: u8, side: u8, sector: u8) -> Option<usize> {
        if track >= self.tracks
            || side >= self.sides
            || sector == 0
            || sector > self.sectors_per_track
        {
            return None;
        }

        let sector_size = usize::from(self.sector_size);
        let linear_track = usize::from(track) * usize::from(self.sides) + usize::from(side);
        let sector_index = usize::from(sector - 1);
        Some((linear_track * usize::from(self.sectors_per_track) + sector_index) * sector_size)
    }

    /// Returns the raw sector payload bytes without the VDK header.
    #[must_use]
    pub fn data(&self) -> &[u8] {
        &self.data
    }

    /// Returns whether the disk contains a DragonDOS directory entry with the
    /// supplied unpadded file name and extension bytes.
    #[must_use]
    pub fn contains_directory_entry(&self, name: &[u8], extension: &[u8]) -> bool {
        let Some(padded_name) = padded_field::<8>(name) else {
            return false;
        };
        let Some(padded_extension) = padded_field::<3>(extension) else {
            return false;
        };

        let sector_size = usize::from(self.sector_size);
        self.data.chunks_exact(sector_size).any(|sector| {
            [0, 1].into_iter().any(|base| {
                (0..10).any(|entry| {
                    let offset = base + entry * 25;
                    sector[offset..offset + 8] == padded_name
                        && sector[offset + 8..offset + 11] == padded_extension
                })
            })
        })
    }

    /// Serializes the disk as a standard VDK image.
    #[must_use]
    pub fn to_vdk_bytes(&self) -> Vec<u8> {
        let mut bytes = Vec::with_capacity(VDK_HEADER_LEN + self.data.len());
        bytes.extend_from_slice(VDK_SIGNATURE);
        bytes.extend_from_slice(&(VDK_HEADER_LEN as u16).to_le_bytes());
        bytes.extend_from_slice(&[0, 0, 0, 0]);
        bytes.push(self.tracks);
        bytes.push(self.sides);
        bytes.extend_from_slice(&[0, 0]);
        bytes.extend_from_slice(&self.data);
        bytes
    }
}

fn padded_field<const N: usize>(bytes: &[u8]) -> Option<[u8; N]> {
    if bytes.len() > N {
        return None;
    }

    let mut padded = [0; N];
    padded[..bytes.len()].copy_from_slice(bytes);
    Some(padded)
}

/// Dragon disk image parse failure.
#[derive(Clone, Debug, Error, PartialEq, Eq)]
pub enum DragonDiskParseError {
    /// The VDK image is too short to contain its fixed header length field.
    #[error("VDK image is too short: got {actual} bytes, need at least {minimum}")]
    TooShort {
        /// Actual byte count.
        actual: usize,
        /// Minimum byte count.
        minimum: usize,
    },
    /// The VDK signature was not present.
    #[error("VDK image has signature 0x{actual:04X}, expected 0x646B")]
    BadSignature {
        /// Observed big-endian signature bytes.
        actual: u16,
    },
    /// The VDK header length does not fit inside the file.
    #[error("VDK header declares {header_len} bytes, but image is only {actual} bytes")]
    HeaderOutOfRange {
        /// Declared header length.
        header_len: usize,
        /// Actual image length.
        actual: usize,
    },
    /// The raw sector payload length does not match the declared geometry.
    #[error(
        "VDK payload is {actual} bytes, expected {expected} for {tracks} tracks, {sides} sides, {sectors_per_track} sectors, {sector_size} bytes per sector"
    )]
    LengthMismatch {
        /// Payload bytes present after the header.
        actual: usize,
        /// Expected payload length.
        expected: usize,
        /// Tracks per side.
        tracks: u8,
        /// Sides.
        sides: u8,
        /// Sectors per track.
        sectors_per_track: u8,
        /// Bytes per sector.
        sector_size: u16,
    },
}

/// Parse a DragonDOS VDK disk image.
///
/// # Errors
///
/// Returns an error if the header is malformed or the payload length does not
/// match the VDK geometry. The supported corpus geometry is the DragonDOS
/// 40-track, one-sided, 18 x 256-byte sector layout.
pub fn parse_vdk(bytes: &[u8]) -> Result<DragonDiskImage, DragonDiskParseError> {
    if bytes.len() < 4 {
        return Err(DragonDiskParseError::TooShort {
            actual: bytes.len(),
            minimum: 4,
        });
    }
    if &bytes[..2] != VDK_SIGNATURE {
        return Err(DragonDiskParseError::BadSignature {
            actual: u16::from_be_bytes([bytes[0], bytes[1]]),
        });
    }

    let header_len = usize::from(u16::from_le_bytes([bytes[2], bytes[3]]));
    if header_len > bytes.len() {
        return Err(DragonDiskParseError::HeaderOutOfRange {
            header_len,
            actual: bytes.len(),
        });
    }

    let tracks = bytes.get(8).copied().unwrap_or(DEFAULT_TRACKS);
    let sides = bytes.get(9).copied().unwrap_or(DEFAULT_SIDES);
    let sectors_per_track = DEFAULT_SECTORS_PER_TRACK;
    let sector_size = DEFAULT_SECTOR_SIZE;
    let expected = usize::from(tracks)
        * usize::from(sides)
        * usize::from(sectors_per_track)
        * usize::from(sector_size);
    let payload = &bytes[header_len..];
    if payload.len() != expected {
        return Err(DragonDiskParseError::LengthMismatch {
            actual: payload.len(),
            expected,
            tracks,
            sides,
            sectors_per_track,
            sector_size,
        });
    }

    Ok(DragonDiskImage {
        tracks,
        sides,
        sectors_per_track,
        sector_size,
        data: payload.into(),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn minimal_vdk() -> Vec<u8> {
        let mut bytes = vec![0; VDK_HEADER_LEN + 40 * 18 * 256];
        bytes[0] = b'd';
        bytes[1] = b'k';
        bytes[2] = VDK_HEADER_LEN as u8;
        bytes[8] = 40;
        bytes[9] = 1;
        bytes[VDK_HEADER_LEN] = 0x55;
        bytes[VDK_HEADER_LEN + 255] = 0xaa;
        bytes
    }

    fn write_directory_entry(
        bytes: &mut [u8],
        sector_offset: usize,
        name: &[u8],
        extension: &[u8],
    ) {
        let entry = VDK_HEADER_LEN + sector_offset;
        bytes[entry..entry + name.len()].copy_from_slice(name);
        bytes[entry + 8..entry + 8 + extension.len()].copy_from_slice(extension);
    }

    #[test]
    fn parses_headered_vdk() {
        let image = parse_vdk(&minimal_vdk()).expect("VDK should parse");

        assert_eq!(image.tracks, 40);
        assert_eq!(image.sides, 1);
        assert_eq!(image.sectors_per_track, 18);
        assert_eq!(image.sector_size, 256);
        assert_eq!(image.sector(0, 0, 1).expect("sector 1")[0], 0x55);
        assert_eq!(image.sector(0, 0, 1).expect("sector 1")[255], 0xaa);
    }

    #[test]
    fn sector_lookup_is_one_based() {
        let image = parse_vdk(&minimal_vdk()).expect("VDK should parse");

        assert!(image.sector(0, 0, 0).is_none());
        assert!(image.sector(0, 0, 19).is_none());
        assert!(image.sector(40, 0, 1).is_none());
    }

    #[test]
    fn mutable_sector_updates_backing_payload() {
        let mut image = parse_vdk(&minimal_vdk()).expect("VDK should parse");

        image.sector_mut(0, 0, 1).expect("sector 1")[1] = 0x42;

        assert_eq!(image.sector(0, 0, 1).expect("sector 1")[1], 0x42);
        assert_eq!(image.data()[1], 0x42);
    }

    #[test]
    fn serializes_mutated_image_as_vdk() {
        let mut image = parse_vdk(&minimal_vdk()).expect("VDK should parse");
        image.sector_mut(0, 0, 1).expect("sector 1")[1] = 0x42;

        let bytes = image.to_vdk_bytes();
        let reparsed = parse_vdk(&bytes).expect("serialized VDK should parse");

        assert_eq!(&bytes[..2], VDK_SIGNATURE);
        assert_eq!(
            u16::from_le_bytes([bytes[2], bytes[3]]),
            VDK_HEADER_LEN as u16
        );
        assert_eq!(bytes[8], 40);
        assert_eq!(bytes[9], 1);
        assert_eq!(reparsed.sector(0, 0, 1).expect("sector 1")[1], 0x42);
    }

    #[test]
    fn finds_dragon_dos_directory_entries() {
        let mut bytes = minimal_vdk();
        write_directory_entry(&mut bytes, 1 + 7 * 25, b"CODX", b"BAS");
        let image = parse_vdk(&bytes).expect("VDK should parse");

        assert!(image.contains_directory_entry(b"CODX", b"BAS"));
        assert!(!image.contains_directory_entry(b"MISS", b"BAS"));
        assert!(!image.contains_directory_entry(b"CODX", b"BIN"));
    }

    #[test]
    fn finds_zero_based_directory_entries() {
        let mut bytes = minimal_vdk();
        write_directory_entry(&mut bytes, 7 * 25, b"CODX", b"BAS");
        let image = parse_vdk(&bytes).expect("VDK should parse");

        assert!(image.contains_directory_entry(b"CODX", b"BAS"));
    }

    #[test]
    fn rejects_overlong_directory_entry_queries() {
        let image = parse_vdk(&minimal_vdk()).expect("VDK should parse");

        assert!(!image.contains_directory_entry(b"TOO-LONG!", b"BAS"));
        assert!(!image.contains_directory_entry(b"CODX", b"LONG"));
    }

    #[test]
    fn rejects_bad_signature() {
        let err = parse_vdk(b"nope").expect_err("bad signature should fail");

        assert!(matches!(err, DragonDiskParseError::BadSignature { .. }));
    }

    #[test]
    fn rejects_payload_length_mismatch() {
        let mut bytes = minimal_vdk();
        bytes.pop();

        let err = parse_vdk(&bytes).expect_err("truncated payload should fail");

        assert!(matches!(err, DragonDiskParseError::LengthMismatch { .. }));
    }
}
