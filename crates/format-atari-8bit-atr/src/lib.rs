//! Atari 8-bit `ATR` disk-image parsing.
//!
//! An `ATR` is a sixteen-byte header followed by a sector-by-sector dump of a
//! floppy. It carries no track, timing or FM-encoding detail, so it describes
//! what a drive would *return* over SIO rather than what is on the media —
//! which is all a drive model needs, and is why copy-protected disks need
//! `ATX` instead.
//!
//! # Header
//!
//! | Offset | Size | Field |
//! |---|---|---|
//! | 0 | 2 | magic `$0296`, the sum of the ASCII bytes of "NICKATARI" |
//! | 2 | 2 | image size in 16-byte paragraphs, low word |
//! | 4 | 2 | sector size, 128 or 256 |
//! | 6 | 1 | image size in paragraphs, high byte |
//! | 7 | 1 | flags; bit 0 write-protects the image |
//! | 8 | 2 | first or typical bad sector |
//! | 10 | 6 | spare, zero |
//!
//! # The size field is not to be trusted
//!
//! Of 803 images carrying the magic, sampled from the TOSEC `[ATR]` set, 483
//! store the size in the documented sixteen-byte paragraphs and **317 store it
//! in eight-byte units** — the size field reads exactly twice the data that
//! follows it. Three agree with neither. That is 40% of the corpus, not a
//! rarity, and the split follows the dumping tool rather than the disk.
//!
//! So the sector count here comes from the length of the file. The header
//! supplies the sector size and the flags; its size field is parsed, reported
//! as [`AtrImage::declared_paragraphs`], and never used to decide what to read.
//!
//! # The first three sectors
//!
//! A double-density disk's first three sectors hold 128 bytes each, because the
//! boot loader in ROM reads them before the drive is told about 256-byte
//! sectors. Images disagree about what to store for them, and all three
//! layouts are in the wild:
//!
//! - **Logical** — 128 bytes each, so the data begins 384 bytes in.
//! - **Physical** — 256 bytes each, the second half padding.
//! - **Padded** — three 128-byte sectors, then 384 bytes of padding.
//!
//! [`AtrImage::parse`] picks between them from the file length and the padding,
//! and [`AtrImage::boot_sector_layout`] reports which it found.

use serde::{Deserialize, Serialize};
use thiserror::Error;

/// The sum of the ASCII bytes of "NICKATARI", which opens every `ATR`.
const MAGIC: u16 = 0x0296;

/// The header that precedes the sector data.
const HEADER_LEN: usize = 16;

/// The first three sectors of a disk always hold 128 bytes, whatever the
/// density, because the boot ROM reads them before the drive is configured.
const BOOT_SECTORS: usize = 3;
const BOOT_SECTOR_SIZE: usize = 128;

/// Why an image could not be read.
#[derive(Debug, Error, PartialEq, Eq)]
pub enum AtrError {
    /// Shorter than the header.
    #[error("ATR image is {0} bytes, too short for a 16-byte header")]
    TooShort(usize),
    /// The first word is not `$0296`.
    #[error("not an ATR image: expected magic $0296, found ${0:04X}")]
    BadMagic(u16),
    /// The sector size is neither 128 nor 256.
    #[error("ATR sector size ${0:04X} is neither 128 nor 256 bytes")]
    BadSectorSize(u16),
    /// Not even one whole sector, or a sector write of the wrong length.
    #[error("ATR holds {0} bytes of sector data, which is not a whole number of {1}-byte sectors")]
    RaggedData(usize, u16),
    /// No sectors at all.
    #[error("ATR image holds no sector data")]
    Empty,
}

/// How an image stores the three 128-byte boot sectors of a double-density
/// disk. Single-density images are always [`Self::Logical`], the distinction
/// having nothing to divide.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum BootSectorLayout {
    /// 128 bytes each, the way the drive returns them.
    Logical,
    /// 256 bytes each, the upper half unused.
    Physical,
    /// 128 bytes each, followed by 384 bytes of padding.
    Padded,
}

/// A parsed `ATR` disk image.
///
/// Carries `Serialize`/`Deserialize` because a disk in a drive is live machine
/// state: sectors written during a session are in here and nowhere else until
/// the image is exported, so a save state has to carry it.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct AtrImage {
    sector_size: u16,
    sector_count: u16,
    flags: u8,
    bad_sector: u16,
    declared_paragraphs: u32,
    boot_sector_layout: BootSectorLayout,
    trailing_bytes: usize,
    /// Sector bytes as they sit in the file, before a double-density logical
    /// image's short boot sectors are widened. The header's size field
    /// describes this, not `data`.
    stored_len: usize,
    /// Sector data, normalised to `sector_size` bytes per sector whatever the
    /// image's boot-sector layout. The first three sectors of a
    /// double-density disk carry their 128 bytes in the low half.
    data: Vec<u8>,
}

impl AtrImage {
    /// Parse an image.
    ///
    /// # Errors
    ///
    /// [`AtrError`] when the bytes are too short, do not carry the magic, name
    /// an impossible sector size, or do not divide into whole sectors.
    pub fn parse(bytes: &[u8]) -> Result<Self, AtrError> {
        if bytes.len() < HEADER_LEN {
            return Err(AtrError::TooShort(bytes.len()));
        }
        let magic = u16::from_le_bytes([bytes[0], bytes[1]]);
        if magic != MAGIC {
            return Err(AtrError::BadMagic(magic));
        }
        let sector_size = u16::from_le_bytes([bytes[4], bytes[5]]);
        if sector_size != 128 && sector_size != 256 {
            return Err(AtrError::BadSectorSize(sector_size));
        }
        let declared_paragraphs =
            u32::from(u16::from_le_bytes([bytes[2], bytes[3]])) | (u32::from(bytes[6]) << 16);
        let flags = bytes[7];
        let bad_sector = u16::from_le_bytes([bytes[8], bytes[9]]);

        let raw = &bytes[HEADER_LEN..];
        if raw.is_empty() {
            return Err(AtrError::Empty);
        }

        let (layout, data, trailing_bytes) = normalise(raw, sector_size)?;
        let sector_count = u16::try_from(data.len() / usize::from(sector_size))
            .map_err(|_| AtrError::RaggedData(raw.len(), sector_size))?;

        Ok(Self {
            sector_size,
            sector_count,
            flags,
            bad_sector,
            declared_paragraphs,
            boot_sector_layout: layout,
            trailing_bytes,
            stored_len: raw.len() - trailing_bytes,
            data,
        })
    }

    /// Bytes per sector: 128 or 256.
    #[must_use]
    pub fn sector_size(&self) -> u16 {
        self.sector_size
    }

    /// How many sectors the image holds, counted from its length.
    #[must_use]
    pub fn sector_count(&self) -> u16 {
        self.sector_count
    }

    /// One sector, numbered from 1 as the drive numbers them. `None` past the
    /// end of the image.
    ///
    /// A double-density image's first three sectors return all
    /// [`Self::sector_size`] bytes; only the low 128 are meaningful, and that
    /// is what a drive would send.
    #[must_use]
    pub fn sector(&self, sector: u16) -> Option<&[u8]> {
        let index = usize::from(sector.checked_sub(1)?);
        let size = usize::from(self.sector_size);
        self.data.get(index * size..(index + 1) * size)
    }

    /// The bytes a drive returns for one sector — 128 for the first three of a
    /// double-density disk, [`Self::sector_size`] for the rest.
    #[must_use]
    pub fn sector_as_read(&self, sector: u16) -> Option<&[u8]> {
        let full = self.sector(sector)?;
        if self.sector_size == 256 && usize::from(sector) <= BOOT_SECTORS {
            Some(&full[..BOOT_SECTOR_SIZE])
        } else {
            Some(full)
        }
    }

    /// Replace one sector's contents. The slice must be the size
    /// [`Self::sector_as_read`] would return for that sector.
    ///
    /// # Errors
    ///
    /// [`AtrError::RaggedData`] if `data` is the wrong length, and
    /// [`AtrError::Empty`] if the sector is past the end of the image.
    pub fn write_sector(&mut self, sector: u16, data: &[u8]) -> Result<(), AtrError> {
        let expected = self.sector_as_read(sector).ok_or(AtrError::Empty)?.len();
        if data.len() != expected {
            return Err(AtrError::RaggedData(
                data.len(),
                u16::try_from(expected).unwrap_or(u16::MAX),
            ));
        }
        let index = usize::from(sector - 1);
        let size = usize::from(self.sector_size);
        self.data[index * size..index * size + data.len()].copy_from_slice(data);
        Ok(())
    }

    /// Whether the image asks not to be written to. This is a request the
    /// image makes, not a property of the media; honouring it is the machine's
    /// business.
    #[must_use]
    pub fn write_protected(&self) -> bool {
        self.flags & 0x01 != 0
    }

    /// The sector the image names as bad, or zero when it names none.
    #[must_use]
    pub fn bad_sector(&self) -> u16 {
        self.bad_sector
    }

    /// The header's size field, in whatever unit its writer used. Reported
    /// rather than trusted — see the module documentation.
    #[must_use]
    pub fn declared_paragraphs(&self) -> u32 {
        self.declared_paragraphs
    }

    /// Bytes left over after the last whole sector.
    ///
    /// A drive reads whole sectors, so a part-written one at the end of a file
    /// is not a sector and is dropped. Two of the 806 TOSEC images sampled end
    /// mid-sector; the rest of the disk reads perfectly well, so this is
    /// reported rather than treated as a reason to refuse the image.
    #[must_use]
    pub fn trailing_bytes(&self) -> usize {
        self.trailing_bytes
    }

    /// Which of the three boot-sector layouts the image turned out to use.
    #[must_use]
    pub fn boot_sector_layout(&self) -> BootSectorLayout {
        self.boot_sector_layout
    }

    /// Whether the size field agrees with the file, read as 16-byte
    /// paragraphs. False for 40% of the TOSEC set, whose writers used 8.
    #[must_use]
    pub fn declared_size_agrees(&self) -> bool {
        self.declared_paragraphs as usize * 16 == self.stored_len
    }
}

/// Normalise the raw sector data to a flat `sector_size` bytes per sector,
/// working out which boot-sector layout the image used on the way.
fn normalise(raw: &[u8], sector_size: u16) -> Result<(BootSectorLayout, Vec<u8>, usize), AtrError> {
    let size = usize::from(sector_size);
    if sector_size == 128 {
        let whole = raw.len() - raw.len() % size;
        if whole == 0 {
            return Err(AtrError::RaggedData(raw.len(), sector_size));
        }
        return Ok((
            BootSectorLayout::Logical,
            raw[..whole].to_vec(),
            raw.len() - whole,
        ));
    }

    let boot_logical = BOOT_SECTORS * BOOT_SECTOR_SIZE; // 384
    let boot_physical = BOOT_SECTORS * size; // 768

    // Physical: every sector is a full 256 bytes, so the whole file divides.
    // Logical: the three boot sectors are short, so the remainder does.
    // Padded looks like physical by length, and is told apart by its second
    // 384 bytes being zero — which for a real 256-byte boot sector they are
    // not, since that half holds the rest of the boot code.
    let padded_or_physical = raw.len().is_multiple_of(size) && raw.len() >= boot_physical;
    if padded_or_physical {
        let padding_is_blank = raw[boot_logical..boot_physical].iter().all(|&b| b == 0);
        let mut data = raw.to_vec();
        let layout = if padding_is_blank {
            // Padded: the boot bytes are already in the low half of each
            // sector; the padding is where the upper half would be.
            BootSectorLayout::Padded
        } else {
            BootSectorLayout::Physical
        };
        data.truncate(raw.len());
        return Ok((layout, data, 0));
    }

    if raw.len() > boot_logical {
        // Logical: widen the three short sectors so every sector is one stride.
        let tail = raw.len() - boot_logical;
        let whole_tail = tail - tail % size;
        let mut data = vec![0u8; BOOT_SECTORS * size + whole_tail];
        for sector in 0..BOOT_SECTORS {
            let from = sector * BOOT_SECTOR_SIZE;
            data[sector * size..sector * size + BOOT_SECTOR_SIZE]
                .copy_from_slice(&raw[from..from + BOOT_SECTOR_SIZE]);
        }
        data[BOOT_SECTORS * size..].copy_from_slice(&raw[boot_logical..boot_logical + whole_tail]);
        return Ok((BootSectorLayout::Logical, data, tail - whole_tail));
    }

    Err(AtrError::RaggedData(raw.len(), sector_size))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn header(paragraphs: u32, sector_size: u16, flags: u8) -> Vec<u8> {
        let mut h = vec![0u8; HEADER_LEN];
        h[0..2].copy_from_slice(&MAGIC.to_le_bytes());
        h[2..4].copy_from_slice(&(paragraphs as u16).to_le_bytes());
        h[4..6].copy_from_slice(&sector_size.to_le_bytes());
        h[6] = (paragraphs >> 16) as u8;
        h[7] = flags;
        h
    }

    fn single_density(sectors: u16) -> Vec<u8> {
        let data_len = usize::from(sectors) * 128;
        let mut image = header((data_len / 16) as u32, 128, 0);
        image.extend(std::iter::repeat_n(0xA5, data_len));
        image
    }

    #[test]
    fn a_single_density_disk_reads_back_its_sectors() {
        let image = AtrImage::parse(&single_density(720)).expect("parses");
        assert_eq!(image.sector_size(), 128);
        assert_eq!(image.sector_count(), 720);
        assert_eq!(image.sector(1).expect("sector 1").len(), 128);
        assert_eq!(image.sector(720).expect("last sector").len(), 128);
        assert!(image.sector(0).is_none(), "sectors count from one");
        assert!(image.sector(721).is_none(), "past the end of the disk");
    }

    /// The sector count comes from the file, not the header. 317 of the 803
    /// TOSEC images carrying the magic write the size in eight-byte units
    /// rather than the documented sixteen, so a reader that believed the
    /// header would look for twice the sectors that are there.
    #[test]
    fn the_size_field_does_not_decide_how_much_to_read() {
        let mut image = single_density(720);
        // Rewrite the size field the way the eight-byte-unit tools do.
        let doubled = (720u32 * 128) / 8;
        image[2..4].copy_from_slice(&(doubled as u16).to_le_bytes());
        image[6] = (doubled >> 16) as u8;

        let atr = AtrImage::parse(&image).expect("parses");
        assert_eq!(atr.sector_count(), 720);
        assert!(!atr.declared_size_agrees());
        assert_eq!(atr.declared_paragraphs(), doubled);
    }

    #[test]
    fn a_short_or_unmarked_image_is_refused() {
        assert_eq!(AtrImage::parse(&[]), Err(AtrError::TooShort(0)));
        assert_eq!(
            AtrImage::parse(&[0u8; HEADER_LEN + 128]),
            Err(AtrError::BadMagic(0))
        );

        let mut odd = header(1, 512, 0);
        odd.extend([0u8; 512]);
        assert_eq!(AtrImage::parse(&odd), Err(AtrError::BadSectorSize(512)));

        let mut ragged = header(1, 128, 0);
        ragged.extend([0u8; 100]);
        assert_eq!(
            AtrImage::parse(&ragged),
            Err(AtrError::RaggedData(100, 128))
        );
    }

    /// A drive reads whole sectors, so a file that stops mid-sector still
    /// offers every sector before it. Two of the 806 TOSEC images sampled end
    /// this way.
    #[test]
    fn a_part_written_last_sector_is_dropped_rather_than_refused() {
        let mut image = single_density(720);
        image.extend([0x11u8; 76]);

        let atr = AtrImage::parse(&image).expect("parses");
        assert_eq!(atr.sector_count(), 720);
        assert_eq!(atr.trailing_bytes(), 76);
        assert!(atr.sector(721).is_none());
    }

    #[test]
    fn the_write_protect_flag_is_read() {
        let mut image = single_density(1);
        image[7] = 0x01;
        assert!(AtrImage::parse(&image).expect("parses").write_protected());
        assert!(
            !AtrImage::parse(&single_density(1))
                .expect("parses")
                .write_protected()
        );
    }

    /// A double-density disk's first three sectors hold 128 bytes, whatever
    /// the rest of the disk does, and an image may store them three ways.
    /// Whichever it used, a drive reads 128 bytes from sector 1 and 256 from
    /// sector 4.
    #[test]
    fn every_double_density_boot_layout_reads_the_same_way() {
        let tail_sectors = 5usize;
        let tail: Vec<u8> = (0..tail_sectors * 256).map(|i| (i % 251) as u8).collect();
        let boot: Vec<u8> = (0..3 * 128).map(|i| (i % 97 + 1) as u8).collect();

        let mut logical = header(0, 256, 0);
        logical.extend(&boot);
        logical.extend(&tail);

        let mut padded = header(0, 256, 0);
        padded.extend(&boot);
        padded.extend(std::iter::repeat_n(0u8, 384));
        padded.extend(&tail);

        let mut physical = header(0, 256, 0);
        for sector in 0..3 {
            physical.extend(&boot[sector * 128..(sector + 1) * 128]);
            physical.extend(std::iter::repeat_n(0xFFu8, 128));
        }
        physical.extend(&tail);

        for (name, bytes, expected) in [
            ("logical", logical, BootSectorLayout::Logical),
            ("padded", padded, BootSectorLayout::Padded),
            ("physical", physical, BootSectorLayout::Physical),
        ] {
            let atr = AtrImage::parse(&bytes).unwrap_or_else(|e| panic!("{name}: {e}"));
            assert_eq!(atr.boot_sector_layout(), expected, "{name}");
            assert_eq!(atr.sector_count(), 3 + tail_sectors as u16, "{name}");
            assert_eq!(
                atr.sector_as_read(1).expect("sector 1"),
                &boot[..128],
                "{name}: the drive reads 128 bytes from a boot sector"
            );
            assert_eq!(
                atr.sector_as_read(4).expect("sector 4").len(),
                256,
                "{name}: and 256 from the rest"
            );
            assert_eq!(
                atr.sector_as_read(4).expect("sector 4"),
                &tail[..256],
                "{name}"
            );
        }
    }

    #[test]
    fn a_sector_can_be_written_back() {
        let mut atr = AtrImage::parse(&single_density(720)).expect("parses");
        let written = [0x5Au8; 128];
        atr.write_sector(360, &written).expect("writes");
        assert_eq!(atr.sector(360).expect("sector 360"), &written);
        assert_eq!(
            atr.write_sector(360, &[0u8; 64]),
            Err(AtrError::RaggedData(64, 128)),
            "a short write is refused rather than padded"
        );
        assert_eq!(atr.write_sector(721, &written), Err(AtrError::Empty));
    }
}
