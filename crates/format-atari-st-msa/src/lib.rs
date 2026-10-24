//! Atari ST `.msa` floppy images (Magic Shadow Archiver).
//!
//! An MSA file is a ten-byte header followed by one block per track and side,
//! in the order track 0 side 0, track 0 side 1, track 1 side 0 and so on. All
//! words are big-endian.
//!
//! | Offset | Size | Field |
//! |---|---|---|
//! | 0 | 2 | magic `$0E0F` |
//! | 2 | 2 | sectors per track |
//! | 4 | 2 | sides minus one (0 or 1) |
//! | 6 | 2 | first track |
//! | 8 | 2 | last track |
//!
//! Each track block starts with a word giving its length. A length equal to a
//! whole track (sectors × 512) means the track is stored raw. Anything shorter
//! is run-length encoded: the byte `$E5` introduces a run as `$E5`, the byte
//! to repeat and a word count; every other byte stands for itself.
//!
//! [`parse`] expands the archive to a raw sector dump in the same order a
//! `.st` file uses. Tracks before the first stored track are left zeroed, so
//! sector offsets stay absolute.

#![forbid(unsafe_code)]

use std::fmt;

/// Bytes per sector.
pub const SECTOR_SIZE: usize = 512;
/// The run marker.
const RUN: u8 = 0xE5;

/// An expanded MSA image.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct MsaImage {
    pub sectors_per_track: usize,
    pub sides: usize,
    pub first_track: usize,
    pub last_track: usize,
    /// Raw sectors for tracks 0 to `last_track`, side-interleaved.
    pub data: Vec<u8>,
}

impl MsaImage {
    /// Tracks the expanded data covers (from track 0).
    #[must_use]
    pub fn tracks(&self) -> usize {
        self.last_track + 1
    }
}

/// Why an archive could not be read.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum MsaError {
    TooShort,
    BadMagic(u16),
    BadGeometry,
    /// A track block runs past the end of the file.
    Truncated {
        track: usize,
        side: usize,
    },
    /// A track expanded to the wrong number of bytes.
    BadTrack {
        track: usize,
        side: usize,
    },
}

impl fmt::Display for MsaError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::TooShort => write!(f, "MSA file shorter than its 10-byte header"),
            Self::BadMagic(magic) => {
                write!(f, "not an MSA file: magic ${magic:04X}, expected $0E0F")
            }
            Self::BadGeometry => write!(f, "MSA header gives an impossible geometry"),
            Self::Truncated { track, side } => {
                write!(
                    f,
                    "MSA track {track} side {side} runs past the end of the file"
                )
            }
            Self::BadTrack { track, side } => {
                write!(
                    f,
                    "MSA track {track} side {side} does not expand to a whole track"
                )
            }
        }
    }
}

impl std::error::Error for MsaError {}

fn be16(bytes: &[u8], offset: usize) -> Option<usize> {
    Some(usize::from(*bytes.get(offset)?) << 8 | usize::from(*bytes.get(offset + 1)?))
}

/// Parse and expand an MSA archive.
///
/// # Errors
///
/// [`MsaError`] for a short or mislabelled file, an impossible geometry, or a
/// track that is cut short or expands to the wrong size.
pub fn parse(bytes: &[u8]) -> Result<MsaImage, MsaError> {
    if bytes.len() < 10 {
        return Err(MsaError::TooShort);
    }
    let magic = be16(bytes, 0).unwrap_or(0);
    if magic != 0x0E0F {
        return Err(MsaError::BadMagic(magic as u16));
    }
    let header = |offset| be16(bytes, offset).unwrap_or(0);
    let sectors_per_track = header(2);
    let sides = header(4) + 1;
    let first_track = header(6);
    let last_track = header(8);
    if !(1..=64).contains(&sectors_per_track)
        || sides > 2
        || first_track > last_track
        || last_track > 255
    {
        return Err(MsaError::BadGeometry);
    }
    let track_size = sectors_per_track * SECTOR_SIZE;
    let mut data = vec![0u8; (last_track + 1) * sides * track_size];
    let mut pos = 10;
    for track in first_track..=last_track {
        for side in 0..sides {
            let length = be16(bytes, pos).ok_or(MsaError::Truncated { track, side })?;
            pos += 2;
            let block = bytes
                .get(pos..pos + length)
                .ok_or(MsaError::Truncated { track, side })?;
            pos += length;
            let start = (track * sides + side) * track_size;
            let out = &mut data[start..start + track_size];
            if length == track_size {
                out.copy_from_slice(block);
            } else {
                expand(block, out).ok_or(MsaError::BadTrack { track, side })?;
            }
        }
    }
    Ok(MsaImage {
        sectors_per_track,
        sides,
        first_track,
        last_track,
        data,
    })
}

/// Expand a run-length track into `out`, which it must fill exactly.
fn expand(block: &[u8], out: &mut [u8]) -> Option<()> {
    let mut written = 0;
    let mut i = 0;
    while i < block.len() {
        if block[i] == RUN {
            let value = *block.get(i + 1)?;
            let count = be16(block, i + 2)?;
            out.get_mut(written..written + count)?.fill(value);
            written += count;
            i += 4;
        } else {
            *out.get_mut(written)? = block[i];
            written += 1;
            i += 1;
        }
    }
    (written == out.len()).then_some(())
}

/// Compress a raw `.st`-order image into an MSA archive, the inverse of
/// [`parse`]. Runs of five or more equal bytes, and every `$E5`, are encoded;
/// a track that would not shrink is stored raw.
#[must_use]
pub fn write(sectors_per_track: usize, sides: usize, data: &[u8]) -> Vec<u8> {
    let track_size = sectors_per_track * SECTOR_SIZE;
    let tracks = data.len() / (track_size * sides);
    let mut out = Vec::new();
    for word in [
        0x0E0F,
        sectors_per_track,
        sides - 1,
        0,
        tracks.saturating_sub(1),
    ] {
        out.extend((word as u16).to_be_bytes());
    }
    for chunk in data.chunks(track_size).take(tracks * sides) {
        let mut packed = Vec::new();
        let mut i = 0;
        while i < chunk.len() {
            let value = chunk[i];
            let run = chunk[i..].iter().take_while(|&&b| b == value).count();
            if run >= 5 || value == RUN {
                packed.extend([RUN, value]);
                packed.extend((run as u16).to_be_bytes());
                i += run;
            } else {
                packed.push(value);
                i += 1;
            }
        }
        let body = if packed.len() < track_size {
            packed
        } else {
            chunk.to_vec()
        };
        out.extend((body.len() as u16).to_be_bytes());
        out.extend(body);
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn disk(sides: usize, spt: usize, tracks: usize) -> Vec<u8> {
        (0..tracks * sides * spt * SECTOR_SIZE)
            .map(|i| match (i / 700) % 3 {
                0 => 0,
                1 => (i % 251) as u8,
                _ => RUN,
            })
            .collect()
    }

    #[test]
    fn write_then_parse_round_trips() {
        let raw = disk(2, 9, 80);
        let msa = write(9, 2, &raw);
        assert!(msa.len() < raw.len(), "the runs compress");
        let image = parse(&msa).expect("parses");
        assert_eq!(image.sides, 2);
        assert_eq!(image.sectors_per_track, 9);
        assert_eq!(image.tracks(), 80);
        assert_eq!(image.data, raw);
    }

    #[test]
    fn an_incompressible_track_is_stored_raw() {
        let raw: Vec<u8> = (0..9 * SECTOR_SIZE)
            .map(|i| (i * 7 % 255) as u8 + 1)
            .collect();
        let msa = write(9, 1, &raw);
        assert_eq!(be16(&msa, 10), Some(9 * SECTOR_SIZE));
        assert_eq!(parse(&msa).expect("parses").data, raw);
    }

    #[test]
    fn a_later_first_track_leaves_earlier_tracks_zero() {
        let mut msa = vec![0x0E, 0x0F, 0, 9, 0, 0, 0, 2, 0, 2];
        msa.extend([0, 4, RUN, 0x55, 0x12, 0x00]);
        let image = parse(&msa).expect("parses");
        assert_eq!(image.data.len(), 3 * 9 * SECTOR_SIZE);
        assert!(image.data[..2 * 9 * SECTOR_SIZE].iter().all(|&b| b == 0));
        assert!(image.data[2 * 9 * SECTOR_SIZE..].iter().all(|&b| b == 0x55));
    }

    #[test]
    fn malformed_archives_are_rejected() {
        assert_eq!(parse(&[0; 5]), Err(MsaError::TooShort));
        assert_eq!(parse(&[0; 10]), Err(MsaError::BadMagic(0)));
        assert_eq!(
            parse(&[0x0E, 0x0F, 0, 0, 0, 0, 0, 0, 0, 0]),
            Err(MsaError::BadGeometry)
        );
        let header = [0x0E, 0x0F, 0, 9, 0, 0, 0, 0, 0, 0];
        assert_eq!(
            parse(&header),
            Err(MsaError::Truncated { track: 0, side: 0 })
        );
        let mut short = header.to_vec();
        short.extend([0, 4, RUN, 0, 0, 10]);
        assert_eq!(parse(&short), Err(MsaError::BadTrack { track: 0, side: 0 }));
        for error in [
            MsaError::TooShort,
            MsaError::BadMagic(1),
            MsaError::BadGeometry,
            MsaError::Truncated { track: 1, side: 0 },
            MsaError::BadTrack { track: 1, side: 1 },
        ] {
            assert!(!error.to_string().is_empty());
        }
    }
}
