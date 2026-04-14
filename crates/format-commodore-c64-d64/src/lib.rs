//! Commodore 1541 `D64` disk-image parsing.
//!
//! `D64` images store decoded 1541 sector data, not live IEC or GCR drive
//! behaviour. This crate is therefore a container/parser layer only.

use thiserror::Error;

const D64_STANDARD_SIZE: usize = 174_848;
const D64_STANDARD_WITH_ERROR_INFO_SIZE: usize = 175_531;
const SECTOR_SIZE: usize = 256;
const DIRECTORY_TRACK: u8 = 18;
const DIRECTORY_START_SECTOR: u8 = 1;
const BAM_TRACK: u8 = 18;
const BAM_SECTOR: u8 = 0;
const TRACK_SECTOR_COUNTS: [u8; 35] = [
    21, 21, 21, 21, 21, 21, 21, 21, 21, 21, 21, 21, 21, 21, 21, 21, 21, 19, 19, 19, 19, 19, 19, 19,
    18, 18, 18, 18, 18, 18, 17, 17, 17, 17, 17,
];

/// One parsed directory entry from a `D64` image.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct D64DirectoryEntry {
    /// Entry name decoded to plain text.
    pub name: String,
    /// CBM DOS file type.
    pub file_type: D64FileType,
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

/// Parsed `D64` directory information.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct D64Directory {
    /// Disk name from the BAM sector.
    pub disk_name: String,
    /// Two-byte disk id, when present.
    pub disk_id: String,
    /// Parsed directory entries.
    pub entries: Vec<D64DirectoryEntry>,
}

/// One loadable PRG extracted from a `D64` image.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct D64Program {
    /// Entry name from the directory.
    pub name: String,
    /// Raw PRG bytes including the two-byte load address prefix.
    pub data: Vec<u8>,
    /// Declared 254-byte block count.
    pub blocks: u16,
}

/// CBM DOS file type.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum D64FileType {
    Del,
    Seq,
    Prg,
    Usr,
    Rel,
    Unknown(u8),
}

/// One `D64` parse failure.
#[derive(Clone, Debug, Error, PartialEq, Eq)]
pub enum D64ParseError {
    /// The image is not one of the currently supported standard sizes.
    #[error("unsupported D64 size {actual} bytes")]
    UnsupportedSize { actual: usize },

    /// One track number was outside the standard 35-track image.
    #[error("invalid track {track}")]
    InvalidTrack { track: u8 },

    /// One sector number was outside the track's valid range.
    #[error("invalid sector {sector} on track {track}")]
    InvalidSector { track: u8, sector: u8 },

    /// One file chain referenced the same sector twice.
    #[error("D64 file chain loops at track {track} sector {sector}")]
    CyclicFileChain { track: u8, sector: u8 },

    /// One directory sector chain referenced the same sector twice.
    #[error("D64 directory chain loops at track {track} sector {sector}")]
    CyclicDirectoryChain { track: u8, sector: u8 },

    /// The image does not contain any loadable PRG entries.
    #[error("D64 image does not contain any PRG directory entries")]
    NoLoadableEntries,

    /// One last data sector declared an invalid byte count.
    #[error("invalid last-sector byte count {count} at track {track} sector {sector}")]
    InvalidLastSectorByteCount { track: u8, sector: u8, count: u8 },
}

/// Parses the BAM and directory sectors from one `D64` image.
///
/// # Errors
///
/// Returns an error if the image size is unsupported or the directory chain is
/// malformed.
pub fn parse_directory(bytes: &[u8]) -> Result<D64Directory, D64ParseError> {
    validate_d64_size(bytes)?;

    let bam = sector(bytes, BAM_TRACK, BAM_SECTOR)?;
    let disk_name = decode_petscii_name(&bam[0x90..0xA0]);
    let disk_id = decode_petscii_name(&bam[0xA2..0xA4]);

    let mut entries = Vec::new();
    let mut visited = vec![false; total_sector_count()];
    let mut track = DIRECTORY_TRACK;
    let mut sector_num = DIRECTORY_START_SECTOR;

    while track != 0 {
        let linear = linear_sector_index(track, sector_num)?;
        if visited[linear] {
            return Err(D64ParseError::CyclicDirectoryChain {
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

            entries.push(D64DirectoryEntry {
                name: decode_petscii_name(&dir_sector[slot + 5..slot + 21]),
                file_type: D64FileType::from_byte(file_type_byte & 0x07),
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

    Ok(D64Directory {
        disk_name,
        disk_id,
        entries,
    })
}

/// Extracts the first PRG entry from one `D64` image.
///
/// # Errors
///
/// Returns an error if the image is malformed or contains no PRG entries.
pub fn extract_first_prg(bytes: &[u8]) -> Result<D64Program, D64ParseError> {
    let directory = parse_directory(bytes)?;
    let entry = directory
        .entries
        .iter()
        .find(|entry| matches!(entry.file_type, D64FileType::Prg))
        .ok_or(D64ParseError::NoLoadableEntries)?;

    let data = read_sector_chain(bytes, entry.start_track, entry.start_sector)?;
    Ok(D64Program {
        name: entry.name.clone(),
        data,
        blocks: entry.blocks,
    })
}

impl D64FileType {
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
) -> Result<Vec<u8>, D64ParseError> {
    let mut data = Vec::with_capacity(SECTOR_SIZE * 4);
    let mut visited = vec![false; total_sector_count()];

    loop {
        let linear = linear_sector_index(track, sector_num)?;
        if visited[linear] {
            return Err(D64ParseError::CyclicFileChain {
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
                return Err(D64ParseError::InvalidLastSectorByteCount {
                    track,
                    sector: sector_num,
                    count: next_sector,
                });
            }
            let used = usize::from(next_sector) - 1;
            if used > 254 {
                return Err(D64ParseError::InvalidLastSectorByteCount {
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

fn validate_d64_size(bytes: &[u8]) -> Result<(), D64ParseError> {
    match bytes.len() {
        D64_STANDARD_SIZE | D64_STANDARD_WITH_ERROR_INFO_SIZE => Ok(()),
        actual => Err(D64ParseError::UnsupportedSize { actual }),
    }
}

fn sector(bytes: &[u8], track: u8, sector_num: u8) -> Result<&[u8], D64ParseError> {
    let offset = sector_offset(track, sector_num)?;
    Ok(&bytes[offset..offset + SECTOR_SIZE])
}

fn sector_offset(track: u8, sector_num: u8) -> Result<usize, D64ParseError> {
    let linear = linear_sector_index(track, sector_num)?;
    Ok(linear * SECTOR_SIZE)
}

fn linear_sector_index(track: u8, sector_num: u8) -> Result<usize, D64ParseError> {
    if track == 0 || usize::from(track) > TRACK_SECTOR_COUNTS.len() {
        return Err(D64ParseError::InvalidTrack { track });
    }

    let sectors_in_track = TRACK_SECTOR_COUNTS[usize::from(track - 1)];
    if sector_num >= sectors_in_track {
        return Err(D64ParseError::InvalidSector {
            track,
            sector: sector_num,
        });
    }

    let prior_sectors: usize = TRACK_SECTOR_COUNTS[..usize::from(track - 1)]
        .iter()
        .map(|&count| usize::from(count))
        .sum();
    Ok(prior_sectors + usize::from(sector_num))
}

const fn total_sector_count() -> usize {
    let mut total = 0usize;
    let mut index = 0usize;
    while index < TRACK_SECTOR_COUNTS.len() {
        total += TRACK_SECTOR_COUNTS[index] as usize;
        index += 1;
    }
    total
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
        vec![0; D64_STANDARD_SIZE]
    }

    fn write_sector(bytes: &mut [u8], track: u8, sector_num: u8, sector: &[u8; SECTOR_SIZE]) {
        let offset = sector_offset(track, sector_num).expect("synthetic sector offset should fit");
        bytes[offset..offset + SECTOR_SIZE].copy_from_slice(sector);
    }

    fn synthetic_image() -> Vec<u8> {
        let mut bytes = blank_image();

        let mut bam = [0u8; SECTOR_SIZE];
        bam[0] = 18;
        bam[1] = 1;
        bam[0x90..0x98].copy_from_slice(b"DEMO DIS");
        bam[0x98] = b'K';
        bam[0xA2..0xA4].copy_from_slice(b"42");
        write_sector(&mut bytes, BAM_TRACK, BAM_SECTOR, &bam);

        let mut directory = [0u8; SECTOR_SIZE];
        directory[2] = 0x82;
        directory[3] = 1;
        directory[4] = 0;
        directory[5..10].copy_from_slice(b"HELLO");
        directory[30..32].copy_from_slice(&(1u16).to_le_bytes());
        write_sector(
            &mut bytes,
            DIRECTORY_TRACK,
            DIRECTORY_START_SECTOR,
            &directory,
        );

        let mut file_sector = [0u8; SECTOR_SIZE];
        file_sector[0] = 0;
        file_sector[1] = 6;
        file_sector[2..7].copy_from_slice(&[0x01, 0x08, 0x11, 0x22, 0x33]);
        write_sector(&mut bytes, 1, 0, &file_sector);

        bytes
    }

    #[test]
    fn parses_directory_from_synthetic_image() {
        let image = synthetic_image();
        let directory = parse_directory(&image).expect("synthetic D64 should parse");

        assert_eq!(directory.disk_name, "DEMO DISK");
        assert_eq!(directory.disk_id, "42");
        assert_eq!(directory.entries.len(), 1);
        assert_eq!(directory.entries[0].name, "HELLO");
        assert_eq!(directory.entries[0].file_type, D64FileType::Prg);
        assert_eq!(directory.entries[0].start_track, 1);
        assert_eq!(directory.entries[0].start_sector, 0);
        assert_eq!(directory.entries[0].blocks, 1);
        assert!(directory.entries[0].closed);
        assert!(!directory.entries[0].locked);
    }

    #[test]
    fn extracts_first_prg_from_synthetic_image() {
        let image = synthetic_image();
        let program = extract_first_prg(&image).expect("synthetic D64 should expose a PRG");

        assert_eq!(program.name, "HELLO");
        assert_eq!(program.blocks, 1);
        assert_eq!(program.data, vec![0x01, 0x08, 0x11, 0x22, 0x33]);
    }

    #[test]
    fn rejects_unsupported_sizes() {
        let err = parse_directory(&[0; 123]).expect_err("bad D64 size must fail");
        assert_eq!(err, D64ParseError::UnsupportedSize { actual: 123 });
    }

    #[test]
    fn rejects_cyclic_file_chain() {
        let mut image = synthetic_image();
        let mut sector = [0u8; SECTOR_SIZE];
        sector[0] = 1;
        sector[1] = 0;
        write_sector(&mut image, 1, 0, &sector);

        let err = extract_first_prg(&image).expect_err("cyclic D64 chain must fail");
        assert_eq!(
            err,
            D64ParseError::CyclicFileChain {
                track: 1,
                sector: 0,
            }
        );
    }
}
