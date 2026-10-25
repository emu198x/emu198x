//! Acorn ADFS floppy images in the old-map formats S, M and L.
//!
//! ADFS (the Advanced Disc Filing System) shipped on the Acorn Electron's
//! Plus 3, the BBC Master and the BBC B with a 1770 interface. Its first discs
//! use the "old map": a two-sector free-space map at the start of the disc and
//! a five-sector root directory after it. Every track holds sixteen 256-byte
//! sectors, numbered 0 to 15, recorded in double density.
//!
//! | Format | Tracks | Sides | Sectors | Image size | Usual extension |
//! |--------|--------|-------|---------|------------|-----------------|
//! | S      | 40     | 1     | 640     | 160 KiB    | `.adf`          |
//! | M      | 80     | 1     | 1280    | 320 KiB    | `.adf`          |
//! | L      | 80     | 2     | 2560    | 640 KiB    | `.adl`          |
//!
//! The images are plain sector dumps. A one-sided image stores its tracks in
//! order. A two-sided `.adl` image interleaves the sides track by track: track
//! 0 side 0, track 0 side 1, track 1 side 0 and so on. That is the physical
//! layout a disc controller sees, so an emulator can map it straight onto its
//! drive.
//!
//! This crate knows nothing about any emulator. It works out an image's
//! format, pads a trimmed image to full size, writes a blank formatted disc
//! and reads the root directory.
//!
//! # The old map
//!
//! Sector 0 holds up to 82 three-byte free-space start sectors, then at
//! `&FC` the disc's size in sectors (three bytes, low first) and at `&FF` a
//! checksum. Sector 1 holds the matching lengths, the disc identifier at
//! `&FB`, the boot option at `&FD`, the length of the free-space list in bytes
//! at `&FE` and its own checksum at `&FF`.
//!
//! A directory is five sectors: a sequence number and the word `Hugo`, up to 47
//! 26-byte entries, and a 53-byte tail holding the directory's name, its
//! parent, its title and the same sequence number and `Hugo` again.
//!
//! Layout cross-checked against the free-space map and root directory of
//! Elkulator 1.0's `WelcomeDisc.adf`, an M-format disc whose checksums this
//! crate reproduces.

#![forbid(unsafe_code)]

use std::fmt;

/// Bytes in a sector.
pub const SECTOR_SIZE: usize = 256;
/// Sectors on each track, numbered from 0.
pub const SECTORS_PER_TRACK: usize = 16;
/// First sector of the root directory.
pub const ROOT_SECTOR: usize = 2;
/// Sectors in a directory.
pub const DIRECTORY_SECTORS: usize = 5;

const DIRECTORY_LEN: usize = DIRECTORY_SECTORS * SECTOR_SIZE;
const ENTRY_LEN: usize = 26;
const MAX_ENTRIES: usize = 47;
const NAME_LEN: usize = 10;
const TITLE_LEN: usize = 19;
/// Offset of the directory tail's name field within a directory.
const TAIL_NAME: usize = 0x4CC;
const TAIL_PARENT: usize = 0x4D6;
const TAIL_TITLE: usize = 0x4D9;
const TAIL_SEQUENCE: usize = 0x4FA;
const TAIL_HUGO: usize = 0x4FB;
const HUGO: &[u8; 4] = b"Hugo";

/// One of the three old-map floppy formats.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Format {
    /// Small: 40 tracks, one side, 160 KiB.
    S,
    /// Medium: 80 tracks, one side, 320 KiB.
    M,
    /// Large: 80 tracks, two sides, 640 KiB.
    L,
}

impl Format {
    /// All three formats, smallest first.
    pub const ALL: [Self; 3] = [Self::S, Self::M, Self::L];

    /// Tracks on each side.
    #[must_use]
    pub const fn tracks(self) -> usize {
        match self {
            Self::S => 40,
            Self::M | Self::L => 80,
        }
    }

    /// Recorded sides.
    #[must_use]
    pub const fn sides(self) -> usize {
        match self {
            Self::S | Self::M => 1,
            Self::L => 2,
        }
    }

    /// Sectors on the whole disc.
    #[must_use]
    pub const fn total_sectors(self) -> usize {
        self.tracks() * self.sides() * SECTORS_PER_TRACK
    }

    /// Bytes in a full image.
    #[must_use]
    pub const fn image_len(self) -> usize {
        self.total_sectors() * SECTOR_SIZE
    }

    /// The usual file extension: `adf` for one side, `adl` for two.
    #[must_use]
    pub const fn extension(self) -> &'static str {
        match self {
            Self::S | Self::M => "adf",
            Self::L => "adl",
        }
    }

    /// The format whose disc holds exactly `sectors` sectors.
    #[must_use]
    pub fn from_total_sectors(sectors: usize) -> Option<Self> {
        Self::ALL.into_iter().find(|f| f.total_sectors() == sectors)
    }
}

/// Why an image could not be read.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Error {
    /// The image holds no bytes.
    Empty,
    /// The image is bigger than any old-map floppy.
    TooLarge {
        /// The image's length in bytes.
        len: usize,
    },
    /// The image is too short to hold a free-space map and root directory.
    TooShort {
        /// The image's length in bytes.
        len: usize,
    },
    /// The root directory does not carry `Hugo` and the same sequence number
    /// at both ends, which ADFS reports as a broken directory.
    NotAnAdfsDirectory,
}

impl fmt::Display for Error {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Empty => write!(f, "the image is empty"),
            Self::TooLarge { len } => write!(
                f,
                "the image is {len} bytes, more than an ADFS L disc's {}",
                Format::L.image_len()
            ),
            Self::TooShort { len } => write!(
                f,
                "the image is {len} bytes, too short for the map and root directory"
            ),
            Self::NotAnAdfsDirectory => {
                write!(f, "the root directory is not a whole ADFS directory")
            }
        }
    }
}

impl std::error::Error for Error {}

/// The old-map checksum of a map sector: bytes 254 down to 0 are summed into
/// an accumulator that starts at 255, adding each carry back in before the
/// next byte. The result belongs in byte 255.
#[must_use]
pub fn map_checksum(sector: &[u8]) -> u8 {
    let mut sum: u32 = 255;
    for &byte in sector.iter().take(SECTOR_SIZE - 1).rev() {
        if sum > 255 {
            sum = (sum & 0xFF) + 1;
        }
        sum += u32::from(byte);
    }
    (sum & 0xFF) as u8
}

/// The disc size the free-space map records, when both map sectors carry a
/// good checksum.
#[must_use]
pub fn map_total_sectors(image: &[u8]) -> Option<usize> {
    if image.len() < 2 * SECTOR_SIZE {
        return None;
    }
    let (s0, s1) = (&image[..SECTOR_SIZE], &image[SECTOR_SIZE..2 * SECTOR_SIZE]);
    if map_checksum(s0) != s0[255] || map_checksum(s1) != s1[255] {
        return None;
    }
    Some(usize::from(s0[0xFC]) | usize::from(s0[0xFD]) << 8 | usize::from(s0[0xFE]) << 16)
}

/// Work out an image's format.
///
/// The free-space map's own size field decides when its checksums are good
/// and it names one of the three formats. Otherwise the image's length does:
/// the smallest format it fits. A trimmed image, cut short after its last used
/// sector, is still recognised.
///
/// # Errors
///
/// [`Error::Empty`] for no bytes and [`Error::TooLarge`] for more than an L
/// disc holds.
pub fn detect(image: &[u8]) -> Result<Format, Error> {
    if image.is_empty() {
        return Err(Error::Empty);
    }
    if image.len() > Format::L.image_len() {
        return Err(Error::TooLarge { len: image.len() });
    }
    if let Some(format) = map_total_sectors(image).and_then(Format::from_total_sectors) {
        return Ok(format);
    }
    Format::ALL
        .into_iter()
        .find(|f| image.len() <= f.image_len())
        .ok_or(Error::TooLarge { len: image.len() })
}

/// The image padded with zeros to its format's full size, so every sector
/// can be read and written.
#[must_use]
pub fn padded(image: &[u8], format: Format) -> Vec<u8> {
    let mut bytes = image.to_vec();
    bytes.resize(format.image_len().max(image.len()), 0);
    bytes
}

/// A blank, formatted disc: an empty free-space map and an empty root
/// directory `$` titled `title` (cut to 19 characters).
#[must_use]
pub fn blank(format: Format, title: &str) -> Vec<u8> {
    let mut image = vec![0u8; format.image_len()];
    let first_free = ROOT_SECTOR + DIRECTORY_SECTORS;
    let free = format.total_sectors() - first_free;
    put24(&mut image[0..3], first_free);
    put24(&mut image[0xFC..0xFF], format.total_sectors());
    put24(&mut image[SECTOR_SIZE..SECTOR_SIZE + 3], free);
    // One free-space entry, three bytes long.
    image[SECTOR_SIZE + 0xFE] = 3;
    image[0xFF] = map_checksum(&image[..SECTOR_SIZE]);
    image[SECTOR_SIZE + 0xFF] = map_checksum(&image[SECTOR_SIZE..2 * SECTOR_SIZE]);

    let dir = &mut image[ROOT_SECTOR * SECTOR_SIZE..][..DIRECTORY_LEN];
    dir[1..5].copy_from_slice(HUGO);
    dir[TAIL_NAME] = b'$';
    put24(&mut dir[TAIL_PARENT..TAIL_PARENT + 3], ROOT_SECTOR);
    let title = title.as_bytes();
    let len = title.len().min(TITLE_LEN);
    dir[TAIL_TITLE..TAIL_TITLE + len].copy_from_slice(&title[..len]);
    if len < TITLE_LEN {
        dir[TAIL_TITLE + len] = b'\r';
    }
    dir[TAIL_HUGO..TAIL_HUGO + 4].copy_from_slice(HUGO);
    image
}

/// One object in a directory.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Entry {
    /// The object's name, without its attribute bits.
    pub name: String,
    /// Load address.
    pub load: u32,
    /// Execution address.
    pub exec: u32,
    /// Length in bytes.
    pub length: u32,
    /// First sector of the object's data.
    pub start_sector: usize,
    /// Readable (`R`).
    pub readable: bool,
    /// Writable (`W`).
    pub writable: bool,
    /// Locked (`L`).
    pub locked: bool,
    /// A directory rather than a file (`D`).
    pub directory: bool,
}

/// A directory read from an image.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Directory {
    /// The directory's own name, `$` for the root.
    pub name: String,
    /// The disc or directory title.
    pub title: String,
    /// The master sequence number.
    pub sequence: u8,
    /// The entries, in the order stored.
    pub entries: Vec<Entry>,
}

/// Read the root directory, sectors 2 to 6.
///
/// # Errors
///
/// [`Error::TooShort`] if the image ends before the directory does, and
/// [`Error::NotAnAdfsDirectory`] if its two ends disagree.
pub fn root_directory(image: &[u8]) -> Result<Directory, Error> {
    let start = ROOT_SECTOR * SECTOR_SIZE;
    let dir = image
        .get(start..start + DIRECTORY_LEN)
        .ok_or(Error::TooShort { len: image.len() })?;
    if &dir[1..5] != HUGO || &dir[TAIL_HUGO..TAIL_HUGO + 4] != HUGO || dir[0] != dir[TAIL_SEQUENCE]
    {
        return Err(Error::NotAnAdfsDirectory);
    }
    let entries = dir[5..5 + MAX_ENTRIES * ENTRY_LEN]
        .as_chunks::<ENTRY_LEN>()
        .0
        .iter()
        .take_while(|e| e[0] != 0)
        .map(entry)
        .collect();
    Ok(Directory {
        name: text(&dir[TAIL_NAME..TAIL_NAME + NAME_LEN]),
        title: text(&dir[TAIL_TITLE..TAIL_TITLE + TITLE_LEN]),
        sequence: dir[0],
        entries,
    })
}

fn entry(e: &[u8; ENTRY_LEN]) -> Entry {
    let bit = |i: usize| e[i] & 0x80 != 0;
    Entry {
        name: text(&e[..NAME_LEN]),
        load: u32::from_le_bytes([e[10], e[11], e[12], e[13]]),
        exec: u32::from_le_bytes([e[14], e[15], e[16], e[17]]),
        length: u32::from_le_bytes([e[18], e[19], e[20], e[21]]),
        start_sector: usize::from(e[22]) | usize::from(e[23]) << 8 | usize::from(e[24]) << 16,
        readable: bit(0),
        writable: bit(1),
        locked: bit(2),
        directory: bit(3),
    }
}

/// Text up to the first control character or space, attribute bits removed.
fn text(bytes: &[u8]) -> String {
    bytes
        .iter()
        .map(|b| b & 0x7F)
        .take_while(|&b| b > b' ')
        .map(char::from)
        .collect()
}

fn put24(out: &mut [u8], value: usize) {
    out[0] = (value & 0xFF) as u8;
    out[1] = ((value >> 8) & 0xFF) as u8;
    out[2] = ((value >> 16) & 0xFF) as u8;
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_formats_have_their_documented_sizes() {
        assert_eq!(Format::S.image_len(), 160 * 1024);
        assert_eq!(Format::M.image_len(), 320 * 1024);
        assert_eq!(Format::L.image_len(), 640 * 1024);
        assert_eq!(Format::L.extension(), "adl");
        assert_eq!(Format::from_total_sectors(1280), Some(Format::M));
        assert_eq!(Format::from_total_sectors(1000), None);
    }

    #[test]
    fn a_blank_disc_has_a_valid_map_and_an_empty_root() {
        for format in Format::ALL {
            let image = blank(format, "EMU198X");
            assert_eq!(image.len(), format.image_len());
            assert_eq!(map_total_sectors(&image), Some(format.total_sectors()));
            assert_eq!(detect(&image), Ok(format));
            let root = root_directory(&image).expect("root");
            assert_eq!(root.name, "$");
            assert_eq!(root.title, "EMU198X");
            assert!(root.entries.is_empty());
            // One free extent from sector 7 to the end of the disc.
            assert_eq!(&image[0..3], &[7, 0, 0]);
            let free = format.total_sectors() - 7;
            assert_eq!(usize::from(image[256]) | usize::from(image[257]) << 8, free);
        }
    }

    #[test]
    fn the_checksum_matches_a_real_map_sector() {
        // Sector 0 of Elkulator 1.0's WelcomeDisc.adf, an M disc: one free
        // extent at &2F6, 1280 sectors, checksum &FD.
        let mut s0 = [0u8; 256];
        s0[0] = 0xF6;
        s0[1] = 0x02;
        s0[0xFD] = 0x05;
        assert_eq!(map_checksum(&s0), 0xFD);
        // Its sector 1: length &20A, disc id &8DA0, boot 2, list end 3: &3F.
        let mut s1 = [0u8; 256];
        s1[0] = 0x0A;
        s1[1] = 0x02;
        s1[0xFB] = 0xA0;
        s1[0xFC] = 0x8D;
        s1[0xFD] = 0x02;
        s1[0xFE] = 0x03;
        assert_eq!(map_checksum(&s1), 0x3F);
    }

    #[test]
    fn a_trimmed_or_unmapped_image_is_sized_by_length() {
        assert_eq!(detect(&[]), Err(Error::Empty));
        assert_eq!(detect(&[0; 1000]), Ok(Format::S));
        assert_eq!(detect(&vec![0; 200 * 1024]), Ok(Format::M));
        assert_eq!(detect(&vec![0; 400 * 1024]), Ok(Format::L));
        assert_eq!(
            detect(&vec![0; 641 * 1024]),
            Err(Error::TooLarge { len: 641 * 1024 })
        );
        // A trimmed L disc still says it is an L disc.
        let mut l = blank(Format::L, "L");
        l.truncate(8 * 1024);
        assert_eq!(detect(&l), Ok(Format::L));
        assert_eq!(padded(&l, Format::L).len(), Format::L.image_len());
    }

    #[test]
    fn root_entries_decode_names_attributes_and_addresses() {
        let mut image = blank(Format::M, "T");
        let e = 2 * 256 + 5;
        image[e..e + 6].copy_from_slice(b"!BOOT\r");
        image[e] |= 0x80; // R
        image[e + 2] |= 0x80; // L
        image[e + 10..e + 14].copy_from_slice(&0x1900u32.to_le_bytes());
        image[e + 14..e + 18].copy_from_slice(&0x8023u32.to_le_bytes());
        image[e + 18..e + 22].copy_from_slice(&0x32u32.to_le_bytes());
        image[e + 22..e + 25].copy_from_slice(&[7, 0, 0]);
        let root = root_directory(&image).expect("root");
        assert_eq!(root.entries.len(), 1);
        let boot = &root.entries[0];
        assert_eq!(boot.name, "!BOOT");
        assert!(boot.readable && boot.locked && !boot.writable && !boot.directory);
        assert_eq!((boot.load, boot.exec, boot.length), (0x1900, 0x8023, 0x32));
        assert_eq!(boot.start_sector, 7);
    }

    #[test]
    fn a_non_adfs_image_is_refused() {
        assert_eq!(root_directory(&[0; 4096]), Err(Error::NotAnAdfsDirectory));
        assert_eq!(root_directory(&[0; 100]), Err(Error::TooShort { len: 100 }));
        assert!(Error::Empty.to_string().contains("empty"));
    }
}
