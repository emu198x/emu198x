//! Commodore 1581 `D81` disk-image parsing.
//!
//! `D81` images store decoded 1581 sector data (the CBM logical scheme the
//! drive presents to the C64: 80 tracks of 40 sectors, 256 bytes each), not
//! live IEC or WD177x flux behaviour. This crate is therefore a
//! container/parser layer only — the mirror of `format-commodore-c64-d64` for
//! the 3.5" drive.
//!
//! Unlike the D64's speed-zoned geometry, the D81 is a flat run of 256-byte
//! blocks in `(track, sector)` order: block `n = (track - 1) * 40 + sector`
//! sits at byte `n * 256`.

use thiserror::Error;

const D81_STANDARD_SIZE: usize = 819_200;
/// Standard image plus a one-byte-per-block error-info map (3200 bytes).
const D81_WITH_ERROR_INFO_SIZE: usize = D81_STANDARD_SIZE + 3_200;
const SECTOR_SIZE: usize = 256;
const TRACK_COUNT: u8 = 80;
const SECTORS_PER_TRACK: u8 = 40;
const DIRECTORY_TRACK: u8 = 40;
const DIRECTORY_START_SECTOR: u8 = 3;
const HEADER_TRACK: u8 = 40;
const HEADER_SECTOR: u8 = 0;

/// One parsed directory entry from a `D81` image.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct D81DirectoryEntry {
    /// Entry name decoded to plain text.
    pub name: String,
    /// CBM DOS file type.
    pub file_type: D81FileType,
    /// First data track.
    pub start_track: u8,
    /// First data sector.
    pub start_sector: u8,
    /// Declared block count in 254-byte blocks.
    pub blocks: u16,
    /// Whether the entry is marked closed.
    pub closed: bool,
    /// Whether the entry is marked locked.
    pub locked: bool,
}

/// Parsed `D81` directory information.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct D81Directory {
    /// Disk name from the header sector.
    pub disk_name: String,
    /// Two-byte disk id.
    pub disk_id: String,
    /// Parsed directory entries.
    pub entries: Vec<D81DirectoryEntry>,
}

/// One loadable PRG extracted from a `D81` image.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct D81Program {
    /// Entry name from the directory.
    pub name: String,
    /// Raw PRG bytes including the two-byte load address prefix.
    pub data: Vec<u8>,
    /// Declared 254-byte block count.
    pub blocks: u16,
}

/// CBM DOS file type.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum D81FileType {
    Del,
    Seq,
    Prg,
    Usr,
    Rel,
    Unknown(u8),
}

/// One `D81` parse failure.
#[derive(Clone, Debug, Error, PartialEq, Eq)]
pub enum D81ParseError {
    /// The image is not one of the currently supported standard sizes.
    #[error("unsupported D81 size {actual} bytes")]
    UnsupportedSize { actual: usize },

    /// One track number was outside the standard 80-track image.
    #[error("invalid track {track}")]
    InvalidTrack { track: u8 },

    /// One sector number was outside the track's valid range.
    #[error("invalid sector {sector} on track {track}")]
    InvalidSector { track: u8, sector: u8 },

    /// One file chain referenced the same sector twice.
    #[error("D81 file chain loops at track {track} sector {sector}")]
    CyclicFileChain { track: u8, sector: u8 },

    /// One directory sector chain referenced the same sector twice.
    #[error("D81 directory chain loops at track {track} sector {sector}")]
    CyclicDirectoryChain { track: u8, sector: u8 },

    /// The image does not contain any loadable PRG entries.
    #[error("D81 image does not contain any PRG directory entries")]
    NoLoadableEntries,

    /// One last data sector declared an invalid byte count.
    #[error("invalid last-sector byte count {count} at track {track} sector {sector}")]
    InvalidLastSectorByteCount { track: u8, sector: u8, count: u8 },
}

/// Parses the header and directory sectors from one `D81` image.
///
/// # Errors
///
/// Returns an error if the image size is unsupported or the directory chain is
/// malformed.
pub fn parse_directory(bytes: &[u8]) -> Result<D81Directory, D81ParseError> {
    validate_d81_size(bytes)?;

    let header = sector(bytes, HEADER_TRACK, HEADER_SECTOR)?;
    // Header block: disk name at $04-$13 ($A0-padded), disk id at $16-$17.
    let disk_name = decode_petscii_name(&header[0x04..0x14]);
    let disk_id = decode_petscii_name(&header[0x16..0x18]);

    let mut entries = Vec::new();
    let mut visited = vec![false; total_sector_count()];
    let mut track = DIRECTORY_TRACK;
    let mut sector_num = DIRECTORY_START_SECTOR;

    while track != 0 {
        let linear = linear_sector_index(track, sector_num)?;
        if visited[linear] {
            return Err(D81ParseError::CyclicDirectoryChain {
                track,
                sector: sector_num,
            });
        }
        visited[linear] = true;

        let dir_sector = sector(bytes, track, sector_num)?;
        for index in 0..8 {
            let slot = index * 32;
            let file_type_byte = dir_sector[slot + 2];
            let start_track = dir_sector[slot + 3];
            let start_sector = dir_sector[slot + 4];
            let blocks = u16::from_le_bytes([dir_sector[slot + 30], dir_sector[slot + 31]]);

            if file_type_byte == 0 || start_track == 0 {
                continue;
            }

            entries.push(D81DirectoryEntry {
                name: decode_petscii_name(&dir_sector[slot + 5..slot + 21]),
                file_type: D81FileType::from_byte(file_type_byte & 0x07),
                start_track,
                start_sector,
                blocks,
                closed: file_type_byte & 0x80 != 0,
                locked: file_type_byte & 0x40 != 0,
            });
        }

        track = dir_sector[0];
        sector_num = dir_sector[1];
    }

    Ok(D81Directory {
        disk_name,
        disk_id,
        entries,
    })
}

/// Extracts the first PRG entry from one `D81` image.
///
/// # Errors
///
/// Returns an error if the image is malformed or contains no PRG entries.
pub fn extract_first_prg(bytes: &[u8]) -> Result<D81Program, D81ParseError> {
    let directory = parse_directory(bytes)?;
    let entry = directory
        .entries
        .iter()
        .find(|entry| matches!(entry.file_type, D81FileType::Prg))
        .ok_or(D81ParseError::NoLoadableEntries)?;

    let data = read_sector_chain(bytes, entry.start_track, entry.start_sector)?;
    Ok(D81Program {
        name: entry.name.clone(),
        data,
        blocks: entry.blocks,
    })
}

/// Returns the number of sectors on one `D81` track (a uniform 40).
///
/// # Errors
///
/// Returns an error if the track is outside the supported 80-track range.
pub fn sectors_in_track(track: u8) -> Result<u8, D81ParseError> {
    if track == 0 || track > TRACK_COUNT {
        return Err(D81ParseError::InvalidTrack { track });
    }
    Ok(SECTORS_PER_TRACK)
}

/// Returns one raw decoded 256-byte sector from a `D81` image.
///
/// # Errors
///
/// Returns an error if the image is not a supported standard size or the
/// track/sector pair is outside the image geometry.
pub fn read_sector(bytes: &[u8], track: u8, sector_num: u8) -> Result<&[u8], D81ParseError> {
    validate_d81_size(bytes)?;
    sector(bytes, track, sector_num)
}

/// Writes one 256-byte sector into a `D81` image in place.
///
/// # Errors
///
/// Returns an error if the image is not a supported standard size or the
/// track/sector pair is outside the image geometry.
pub fn write_sector(
    bytes: &mut [u8],
    track: u8,
    sector_num: u8,
    data: &[u8; SECTOR_SIZE],
) -> Result<(), D81ParseError> {
    validate_d81_size(bytes)?;
    let offset = sector_offset(track, sector_num)?;
    bytes[offset..offset + SECTOR_SIZE].copy_from_slice(data);
    Ok(())
}

impl D81FileType {
    const fn from_byte(value: u8) -> Self {
        match value {
            0 => Self::Del,
            1 => Self::Seq,
            2 => Self::Prg,
            3 => Self::Usr,
            4 => Self::Rel,
            other => Self::Unknown(other),
        }
    }
}

fn read_sector_chain(
    bytes: &[u8],
    mut track: u8,
    mut sector_num: u8,
) -> Result<Vec<u8>, D81ParseError> {
    let mut data = Vec::with_capacity(SECTOR_SIZE * 4);
    let mut visited = vec![false; total_sector_count()];

    loop {
        let linear = linear_sector_index(track, sector_num)?;
        if visited[linear] {
            return Err(D81ParseError::CyclicFileChain {
                track,
                sector: sector_num,
            });
        }
        visited[linear] = true;

        let block = sector(bytes, track, sector_num)?;
        let next_track = block[0];
        let next_sector = block[1];
        if next_track == 0 {
            if next_sector == 0 {
                return Err(D81ParseError::InvalidLastSectorByteCount {
                    track,
                    sector: sector_num,
                    count: next_sector,
                });
            }
            let used = usize::from(next_sector) - 1;
            if used > 254 {
                return Err(D81ParseError::InvalidLastSectorByteCount {
                    track,
                    sector: sector_num,
                    count: next_sector,
                });
            }
            data.extend_from_slice(&block[2..2 + used]);
            return Ok(data);
        }

        data.extend_from_slice(&block[2..]);
        track = next_track;
        sector_num = next_sector;
    }
}

fn validate_d81_size(bytes: &[u8]) -> Result<(), D81ParseError> {
    match bytes.len() {
        D81_STANDARD_SIZE | D81_WITH_ERROR_INFO_SIZE => Ok(()),
        actual => Err(D81ParseError::UnsupportedSize { actual }),
    }
}

fn sector(bytes: &[u8], track: u8, sector_num: u8) -> Result<&[u8], D81ParseError> {
    let offset = sector_offset(track, sector_num)?;
    Ok(&bytes[offset..offset + SECTOR_SIZE])
}

fn sector_offset(track: u8, sector_num: u8) -> Result<usize, D81ParseError> {
    let linear = linear_sector_index(track, sector_num)?;
    Ok(linear * SECTOR_SIZE)
}

fn linear_sector_index(track: u8, sector_num: u8) -> Result<usize, D81ParseError> {
    if track == 0 || track > TRACK_COUNT {
        return Err(D81ParseError::InvalidTrack { track });
    }
    if sector_num >= SECTORS_PER_TRACK {
        return Err(D81ParseError::InvalidSector {
            track,
            sector: sector_num,
        });
    }
    Ok(usize::from(track - 1) * usize::from(SECTORS_PER_TRACK) + usize::from(sector_num))
}

const fn total_sector_count() -> usize {
    TRACK_COUNT as usize * SECTORS_PER_TRACK as usize
}

fn decode_petscii_name(bytes: &[u8]) -> String {
    let mut text = String::with_capacity(bytes.len());
    for &byte in bytes {
        let ch = match byte {
            0x00 | 0xA0 | 0x20 => ' ',
            0x30..=0x39 | 0x41..=0x5A => char::from(byte),
            0x61..=0x7A => char::from(byte),
            0xC1..=0xDA => char::from(byte - 0x80),
            0xE1..=0xFA => char::from(byte - 0x80),
            _ => '?',
        };
        text.push(ch);
    }
    text.trim_end().to_owned()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn blank_image() -> Vec<u8> {
        vec![0; D81_STANDARD_SIZE]
    }

    fn put_sector(bytes: &mut [u8], track: u8, sector_num: u8, data: &[u8; SECTOR_SIZE]) {
        write_sector(bytes, track, sector_num, data).expect("synthetic sector should write");
    }

    fn synthetic_image() -> Vec<u8> {
        let mut bytes = blank_image();

        // Header block (40/0): link to first dir sector, disk name + id.
        let mut header = [0u8; SECTOR_SIZE];
        header[0] = DIRECTORY_TRACK;
        header[1] = DIRECTORY_START_SECTOR;
        header[2] = b'D';
        header[0x04..0x0D].copy_from_slice(b"DEMO DISK");
        for byte in &mut header[0x0D..0x14] {
            *byte = 0xA0;
        }
        header[0x16..0x18].copy_from_slice(b"42");
        put_sector(&mut bytes, HEADER_TRACK, HEADER_SECTOR, &header);

        // One directory sector (40/3) with a single PRG entry pointing at 1/0.
        let mut directory = [0u8; SECTOR_SIZE];
        directory[2] = 0x82; // closed PRG
        directory[3] = 1; // start track
        directory[4] = 0; // start sector
        directory[5..10].copy_from_slice(b"HELLO");
        for byte in &mut directory[10..21] {
            *byte = 0xA0;
        }
        directory[30] = 1; // one block
        put_sector(
            &mut bytes,
            DIRECTORY_TRACK,
            DIRECTORY_START_SECTOR,
            &directory,
        );

        // The PRG's single data sector (1/0): last sector, three payload bytes.
        let mut prg = [0u8; SECTOR_SIZE];
        prg[0] = 0; // last sector
        prg[1] = 4; // 3 payload bytes used (count - 1)
        prg[2] = 0x01;
        prg[3] = 0x08; // load address $0801
        prg[4] = 0xAB;
        put_sector(&mut bytes, 1, 0, &prg);

        bytes
    }

    #[test]
    fn standard_size_validates() {
        assert!(read_sector(&blank_image(), 1, 0).is_ok());
    }

    #[test]
    fn rejects_wrong_size() {
        let err = read_sector(&vec![0; 1000], 1, 0).expect_err("wrong size must fail");
        assert_eq!(err, D81ParseError::UnsupportedSize { actual: 1000 });
    }

    #[test]
    fn sector_offset_is_flat_track_major() {
        // Block n = (track-1)*40 + sector, at byte n*256.
        assert_eq!(sector_offset(1, 0).expect("valid"), 0);
        assert_eq!(sector_offset(1, 39).expect("valid"), 39 * 256);
        assert_eq!(sector_offset(2, 0).expect("valid"), 40 * 256);
        assert_eq!(sector_offset(80, 39).expect("valid"), (79 * 40 + 39) * 256);
    }

    #[test]
    fn rejects_out_of_range_track_and_sector() {
        assert_eq!(
            linear_sector_index(0, 0).expect_err("track 0 invalid"),
            D81ParseError::InvalidTrack { track: 0 }
        );
        assert_eq!(
            linear_sector_index(81, 0).expect_err("track 81 invalid"),
            D81ParseError::InvalidTrack { track: 81 }
        );
        assert_eq!(
            linear_sector_index(1, 40).expect_err("sector 40 invalid"),
            D81ParseError::InvalidSector {
                track: 1,
                sector: 40
            }
        );
    }

    #[test]
    fn parses_header_and_directory() {
        let image = synthetic_image();
        let dir = parse_directory(&image).expect("directory parses");
        assert_eq!(dir.disk_name, "DEMO DISK");
        assert_eq!(dir.disk_id, "42");
        assert_eq!(dir.entries.len(), 1);
        assert_eq!(dir.entries[0].name, "HELLO");
        assert_eq!(dir.entries[0].file_type, D81FileType::Prg);
        assert!(dir.entries[0].closed);
    }

    #[test]
    fn extracts_first_prg() {
        let image = synthetic_image();
        let prg = extract_first_prg(&image).expect("PRG extracts");
        assert_eq!(prg.name, "HELLO");
        assert_eq!(prg.data, vec![0x01, 0x08, 0xAB]);
    }

    #[test]
    fn write_then_read_round_trips() {
        let mut image = blank_image();
        let mut payload = [0u8; SECTOR_SIZE];
        payload[0] = 0xCA;
        payload[255] = 0xFE;
        write_sector(&mut image, 40, 17, &payload).expect("write");
        assert_eq!(read_sector(&image, 40, 17).expect("read back"), &payload);
    }
}
