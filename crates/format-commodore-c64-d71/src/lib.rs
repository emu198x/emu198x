//! Commodore 1571 `D71` disk-image parsing.
//!
//! A `D71` is a double-sided `D64`: 70 logical tracks (side 0 = tracks 1-35,
//! side 1 = tracks 36-70) storing decoded 1541 sector data, not live IEC or GCR
//! drive behaviour. This crate is therefore a container/parser layer only, the
//! double-sided counterpart to `format-commodore-c64-d64`.

use thiserror::Error;

const D71_STANDARD_SIZE: usize = 349_696;
const D71_STANDARD_WITH_ERROR_INFO_SIZE: usize = 351_062;
const SECTOR_SIZE: usize = 256;
const DIRECTORY_TRACK: u8 = 18;
const DIRECTORY_START_SECTOR: u8 = 1;
const BAM_TRACK: u8 = 18;
const BAM_SECTOR: u8 = 0;
/// Per-track sector counts. The 35-track 1541 zone pattern (21/19/18/17)
/// repeated for the second side: track 36 mirrors track 1 (21 sectors), …
/// track 70 mirrors track 35 (17 sectors).
const TRACK_SECTOR_COUNTS: [u8; 70] = [
    21, 21, 21, 21, 21, 21, 21, 21, 21, 21, 21, 21, 21, 21, 21, 21, 21, 19, 19, 19, 19, 19, 19, 19,
    18, 18, 18, 18, 18, 18, 17, 17, 17, 17, 17, // side 0 (tracks 1-35)
    21, 21, 21, 21, 21, 21, 21, 21, 21, 21, 21, 21, 21, 21, 21, 21, 21, 19, 19, 19, 19, 19, 19, 19,
    18, 18, 18, 18, 18, 18, 17, 17, 17, 17, 17, // side 1 (tracks 36-70)
];

/// One parsed directory entry from a `D71` image.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct D71DirectoryEntry {
    /// Entry name decoded to plain text.
    pub name: String,
    /// CBM DOS file type.
    pub file_type: D71FileType,
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

/// Parsed `D71` directory information.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct D71Directory {
    /// Disk name from the BAM sector.
    pub disk_name: String,
    /// Two-byte disk id, when present.
    pub disk_id: String,
    /// Parsed directory entries.
    pub entries: Vec<D71DirectoryEntry>,
}

/// One loadable PRG extracted from a `D71` image.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct D71Program {
    /// Entry name from the directory.
    pub name: String,
    /// Raw PRG bytes including the two-byte load address prefix.
    pub data: Vec<u8>,
    /// Declared 254-byte block count.
    pub blocks: u16,
}

/// CBM DOS file type.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum D71FileType {
    Del,
    Seq,
    Prg,
    Usr,
    Rel,
    Unknown(u8),
}

/// One `D71` parse failure.
#[derive(Clone, Debug, Error, PartialEq, Eq)]
pub enum D71ParseError {
    /// The image is not one of the currently supported standard sizes.
    #[error("unsupported D71 size {actual} bytes")]
    UnsupportedSize { actual: usize },

    /// One track number was outside the standard 70-track image.
    #[error("invalid track {track}")]
    InvalidTrack { track: u8 },

    /// One sector number was outside the track's valid range.
    #[error("invalid sector {sector} on track {track}")]
    InvalidSector { track: u8, sector: u8 },

    /// One file chain referenced the same sector twice.
    #[error("D71 file chain loops at track {track} sector {sector}")]
    CyclicFileChain { track: u8, sector: u8 },

    /// One directory sector chain referenced the same sector twice.
    #[error("D71 directory chain loops at track {track} sector {sector}")]
    CyclicDirectoryChain { track: u8, sector: u8 },

    /// The image does not contain any loadable PRG entries.
    #[error("D71 image does not contain any PRG directory entries")]
    NoLoadableEntries,

    /// One last data sector declared an invalid byte count.
    #[error("invalid last-sector byte count {count} at track {track} sector {sector}")]
    InvalidLastSectorByteCount { track: u8, sector: u8, count: u8 },
}

/// Parses the BAM and directory sectors from one `D71` image.
///
/// # Errors
///
/// Returns an error if the image size is unsupported or the directory chain is
/// malformed.
pub fn parse_directory(bytes: &[u8]) -> Result<D71Directory, D71ParseError> {
    validate_d71_size(bytes)?;

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
            return Err(D71ParseError::CyclicDirectoryChain {
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

            entries.push(D71DirectoryEntry {
                name: decode_petscii_name(&dir_sector[slot + 5..slot + 21]),
                file_type: D71FileType::from_byte(file_type_byte & 0x07),
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

    Ok(D71Directory {
        disk_name,
        disk_id,
        entries,
    })
}

/// Extracts the first PRG entry from one `D71` image.
///
/// # Errors
///
/// Returns an error if the image is malformed or contains no PRG entries.
pub fn extract_first_prg(bytes: &[u8]) -> Result<D71Program, D71ParseError> {
    let directory = parse_directory(bytes)?;
    let entry = directory
        .entries
        .iter()
        .find(|entry| matches!(entry.file_type, D71FileType::Prg))
        .ok_or(D71ParseError::NoLoadableEntries)?;

    let data = read_sector_chain(bytes, entry.start_track, entry.start_sector)?;
    Ok(D71Program {
        name: entry.name.clone(),
        data,
        blocks: entry.blocks,
    })
}

/// Returns the number of sectors on one standard 70-track `D71` track.
///
/// # Errors
///
/// Returns an error if the track is outside the supported 70-track range.
pub fn sectors_in_track(track: u8) -> Result<u8, D71ParseError> {
    if track == 0 || usize::from(track) > TRACK_SECTOR_COUNTS.len() {
        return Err(D71ParseError::InvalidTrack { track });
    }

    Ok(TRACK_SECTOR_COUNTS[usize::from(track - 1)])
}

/// Returns one raw decoded 256-byte sector from a `D71` image.
///
/// # Errors
///
/// Returns an error if the image is not a supported standard size or the
/// track/sector pair is outside the image geometry.
pub fn read_sector(bytes: &[u8], track: u8, sector_num: u8) -> Result<&[u8], D71ParseError> {
    validate_d71_size(bytes)?;
    sector(bytes, track, sector_num)
}

/// Writes one 256-byte sector into a `D71` image in place.
///
/// The sector-level counterpart to [`read_sector`]: the write-back flush
/// GCR-decodes each modified track to 256-byte sectors and lands them here.
/// Archive images are mounted read-only and never reach this path; see
/// `knowledge/decisions/disk-save-write-back.md`.
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
) -> Result<(), D71ParseError> {
    validate_d71_size(bytes)?;
    let offset = sector_offset(track, sector_num)?;
    bytes[offset..offset + SECTOR_SIZE].copy_from_slice(data);
    Ok(())
}

impl D71FileType {
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
) -> Result<Vec<u8>, D71ParseError> {
    let mut data = Vec::with_capacity(SECTOR_SIZE * 4);
    let mut visited = vec![false; total_sector_count()];

    loop {
        let linear = linear_sector_index(track, sector_num)?;
        if visited[linear] {
            return Err(D71ParseError::CyclicFileChain {
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
                return Err(D71ParseError::InvalidLastSectorByteCount {
                    track,
                    sector: sector_num,
                    count: next_sector,
                });
            }
            let used = usize::from(next_sector) - 1;
            if used > 254 {
                return Err(D71ParseError::InvalidLastSectorByteCount {
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

fn validate_d71_size(bytes: &[u8]) -> Result<(), D71ParseError> {
    match bytes.len() {
        D71_STANDARD_SIZE | D71_STANDARD_WITH_ERROR_INFO_SIZE => Ok(()),
        actual => Err(D71ParseError::UnsupportedSize { actual }),
    }
}

fn sector(bytes: &[u8], track: u8, sector_num: u8) -> Result<&[u8], D71ParseError> {
    let offset = sector_offset(track, sector_num)?;
    Ok(&bytes[offset..offset + SECTOR_SIZE])
}

fn sector_offset(track: u8, sector_num: u8) -> Result<usize, D71ParseError> {
    let linear = linear_sector_index(track, sector_num)?;
    Ok(linear * SECTOR_SIZE)
}

fn linear_sector_index(track: u8, sector_num: u8) -> Result<usize, D71ParseError> {
    if track == 0 || usize::from(track) > TRACK_SECTOR_COUNTS.len() {
        return Err(D71ParseError::InvalidTrack { track });
    }

    let sectors_in_track = TRACK_SECTOR_COUNTS[usize::from(track - 1)];
    if sector_num >= sectors_in_track {
        return Err(D71ParseError::InvalidSector {
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
        vec![0; D71_STANDARD_SIZE]
    }

    fn put_sector(bytes: &mut [u8], track: u8, sector_num: u8, sector: &[u8; SECTOR_SIZE]) {
        write_sector(bytes, track, sector_num, sector).expect("synthetic sector should write");
    }

    fn synthetic_image() -> Vec<u8> {
        let mut bytes = blank_image();

        let mut bam = [0u8; SECTOR_SIZE];
        bam[0] = 18;
        bam[1] = 1;
        bam[0x90..0x98].copy_from_slice(b"DEMO DIS");
        bam[0x98] = b'K';
        bam[0xA2..0xA4].copy_from_slice(b"42");
        put_sector(&mut bytes, BAM_TRACK, BAM_SECTOR, &bam);

        let mut directory = [0u8; SECTOR_SIZE];
        directory[2] = 0x82;
        directory[3] = 1;
        directory[4] = 0;
        directory[5..10].copy_from_slice(b"HELLO");
        directory[30..32].copy_from_slice(&(1u16).to_le_bytes());
        put_sector(
            &mut bytes,
            DIRECTORY_TRACK,
            DIRECTORY_START_SECTOR,
            &directory,
        );

        let mut file_sector = [0u8; SECTOR_SIZE];
        file_sector[0] = 0;
        file_sector[1] = 6;
        file_sector[2..7].copy_from_slice(&[0x01, 0x08, 0x11, 0x22, 0x33]);
        put_sector(&mut bytes, 1, 0, &file_sector);

        bytes
    }

    #[test]
    fn parses_directory_from_synthetic_image() {
        let image = synthetic_image();
        let directory = parse_directory(&image).expect("synthetic D71 should parse");

        assert_eq!(directory.disk_name, "DEMO DISK");
        assert_eq!(directory.disk_id, "42");
        assert_eq!(directory.entries.len(), 1);
        assert_eq!(directory.entries[0].name, "HELLO");
        assert_eq!(directory.entries[0].file_type, D71FileType::Prg);
        assert_eq!(directory.entries[0].start_track, 1);
        assert_eq!(directory.entries[0].start_sector, 0);
        assert_eq!(directory.entries[0].blocks, 1);
        assert!(directory.entries[0].closed);
        assert!(!directory.entries[0].locked);
    }

    #[test]
    fn extracts_first_prg_from_synthetic_image() {
        let image = synthetic_image();
        let program = extract_first_prg(&image).expect("synthetic D71 should expose a PRG");

        assert_eq!(program.name, "HELLO");
        assert_eq!(program.blocks, 1);
        assert_eq!(program.data, vec![0x01, 0x08, 0x11, 0x22, 0x33]);
    }

    #[test]
    fn exposes_double_sided_track_geometry() {
        // Side 0 (1-35) mirrors the 1541 zones.
        assert_eq!(sectors_in_track(1), Ok(21));
        assert_eq!(sectors_in_track(18), Ok(19));
        assert_eq!(sectors_in_track(25), Ok(18));
        assert_eq!(sectors_in_track(31), Ok(17));
        // Side 1 (36-70) repeats the same pattern, offset by 35.
        assert_eq!(sectors_in_track(36), Ok(21));
        assert_eq!(sectors_in_track(53), Ok(19));
        assert_eq!(sectors_in_track(60), Ok(18));
        assert_eq!(sectors_in_track(66), Ok(17));
        assert_eq!(sectors_in_track(70), Ok(17));
        assert_eq!(
            sectors_in_track(71),
            Err(D71ParseError::InvalidTrack { track: 71 })
        );
    }

    #[test]
    fn side_two_sector_offsets_follow_both_sides() {
        // Track 36 sector 0 is the first sector of side 1, i.e. immediately
        // after all 683 sectors of side 0.
        let mut image = blank_image();
        let mut data = [0u8; SECTOR_SIZE];
        data[0] = 0x5A;
        write_sector(&mut image, 36, 0, &data).expect("side-1 track 36 is valid");
        let read = read_sector(&image, 36, 0).expect("side-1 sector reads back");
        assert_eq!(read[0], 0x5A);
        assert_eq!(image[683 * SECTOR_SIZE], 0x5A);
    }

    #[test]
    fn reads_a_file_chain_that_crosses_the_side_boundary() {
        // A PRG whose first block is on side 0 (track 1) and whose second block
        // is on side 1 (track 36) — the distinctive double-sided case.
        let mut image = synthetic_image();

        let mut first = [0u8; SECTOR_SIZE];
        first[0] = 36; // next track on side 1
        first[1] = 0;
        first[2..4].copy_from_slice(&[0x01, 0x08]);
        first[4..].fill(0xAA);
        put_sector(&mut image, 1, 0, &first);

        let mut second = [0u8; SECTOR_SIZE];
        second[0] = 0; // last block
        second[1] = 3; // two used bytes
        second[2] = 0xBB;
        second[3] = 0xCC;
        put_sector(&mut image, 36, 0, &second);

        let program = extract_first_prg(&image).expect("cross-side chain should read");
        assert_eq!(program.data.len(), 254 + 2);
        assert_eq!(&program.data[..2], &[0x01, 0x08]);
        assert_eq!(&program.data[254..], &[0xBB, 0xCC]);
    }

    #[test]
    fn write_sector_rejects_out_of_range_sector() {
        let mut image = blank_image();
        let data = [0u8; SECTOR_SIZE];
        // Track 31 has only 17 sectors (0..=16); sector 20 is out of range.
        let err = write_sector(&mut image, 31, 20, &data).expect_err("must reject bad sector");
        assert_eq!(
            err,
            D71ParseError::InvalidSector {
                track: 31,
                sector: 20
            }
        );
    }

    #[test]
    fn rejects_unsupported_sizes() {
        // A single-sided D64-sized image is not a valid D71.
        let err = parse_directory(&[0; 174_848]).expect_err("bad D71 size must fail");
        assert_eq!(err, D71ParseError::UnsupportedSize { actual: 174_848 });
    }

    #[test]
    fn accepts_the_error_info_size() {
        let mut image = vec![0u8; D71_STANDARD_WITH_ERROR_INFO_SIZE];
        let mut bam = [0u8; SECTOR_SIZE];
        bam[0x90..0x94].copy_from_slice(b"DISK");
        write_sector(&mut image, BAM_TRACK, BAM_SECTOR, &bam).expect("BAM writes");
        let directory = parse_directory(&image).expect("error-info-sized D71 should parse");
        assert_eq!(directory.disk_name, "DISK");
    }

    #[test]
    fn rejects_cyclic_file_chain() {
        let mut image = synthetic_image();
        let mut sector = [0u8; SECTOR_SIZE];
        sector[0] = 1;
        sector[1] = 0;
        put_sector(&mut image, 1, 0, &sector);

        let err = extract_first_prg(&image).expect_err("cyclic D71 chain must fail");
        assert_eq!(
            err,
            D71ParseError::CyclicFileChain {
                track: 1,
                sector: 0,
            }
        );
    }
}
