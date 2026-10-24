//! The `.mdr` Microdrive cartridge image.
//!
//! A Microdrive cartridge is an endless loop of tape holding up to 254
//! sectors. An `.mdr` file stores the sectors in tape order, 543 bytes
//! each, followed by one write-protect byte. It leaves out what the
//! drive synthesises: the preambles of ten `$00` and two `$FF` bytes
//! before each block, and the gaps between them.
//!
//! | Offset in a sector | Length | Contents |
//! |---|---|---|
//! | 0 | 15 | Sector header: flag, sector number, two spare bytes, cartridge name, checksum |
//! | 15 | 15 | Record header: flag, record number, length, file name, checksum |
//! | 30 | 512 | Data |
//! | 542 | 1 | Data checksum |
//!
//! A cartridge holds fewer than 254 sectors when its tape is shorter, so
//! a file is any whole number of sectors from 10 to 254, with or without
//! the write-protect byte. That byte is non-zero when the cartridge is
//! protected.
//!
//! The layout, the length rules and the write-protect sense follow
//! libspectrum's `microdrive.c` (GPL-2, used for facts only), which is
//! what Fuse reads and writes. The checksum follows the Interface 1
//! ROM's own loop, which libspectrum quotes. 198x's
//! `interface1-reference.md` gives the opposite write-protect sense
//! (`$00` protected); libspectrum's is the one the files in circulation
//! were written with.
//!
//! This crate depends on nothing in the emulator.

/// Bytes in a sector header, and in a record header.
pub const HEADER_LEN: usize = 15;
/// Bytes of data in a sector.
pub const DATA_LEN: usize = 512;
/// Bytes in one sector of an `.mdr` file.
pub const SECTOR_LEN: usize = HEADER_LEN + HEADER_LEN + DATA_LEN + 1;
/// The most sectors a cartridge holds.
pub const MAX_SECTORS: usize = 254;
/// The fewest sectors libspectrum accepts in a file.
pub const MIN_SECTORS: usize = 10;

/// Why bytes are not an `.mdr` image.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum MdrError {
    /// Fewer than [`MIN_SECTORS`] sectors.
    TooShort {
        /// The length supplied.
        len: usize,
    },
    /// More than [`MAX_SECTORS`] sectors.
    TooLong {
        /// The length supplied.
        len: usize,
    },
    /// Not a whole number of sectors, with or without the write-protect
    /// byte.
    Misaligned {
        /// The length supplied.
        len: usize,
    },
}

impl std::fmt::Display for MdrError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::TooShort { len } => write!(
                f,
                "a {len}-byte .mdr image holds fewer than {MIN_SECTORS} sectors"
            ),
            Self::TooLong { len } => write!(
                f,
                "a {len}-byte .mdr image holds more than {MAX_SECTORS} sectors"
            ),
            Self::Misaligned { len } => write!(
                f,
                "a {len}-byte .mdr image is not a whole number of {SECTOR_LEN}-byte sectors"
            ),
        }
    }
}

impl std::error::Error for MdrError {}

/// A sector's header, as the Interface 1 writes it when formatting.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SectorHeader {
    /// `HDFLAG`: bit 0 is set on a sector header.
    pub flag: u8,
    /// `HDNUMB`: the sector number, counting down from 254 to 1.
    pub number: u8,
    /// `HDNAME`: the cartridge name, padded with spaces.
    pub name: [u8; 10],
    /// Whether `HDCHK` matches the other 14 bytes.
    pub checksum_ok: bool,
}

/// A Microdrive cartridge image.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct MdrImage {
    data: Vec<u8>,
    /// Whether the cartridge's write-protect tab is broken off.
    pub write_protected: bool,
}

impl MdrImage {
    /// Reads an `.mdr` file.
    ///
    /// # Errors
    ///
    /// Returns an error when the length is not 10 to 254 whole sectors,
    /// optionally followed by the write-protect byte.
    pub fn parse(bytes: &[u8]) -> Result<Self, MdrError> {
        let len = bytes.len();
        let tail = len % SECTOR_LEN;
        if tail > 1 {
            return Err(MdrError::Misaligned { len });
        }
        let sectors = len / SECTOR_LEN;
        if sectors < MIN_SECTORS {
            return Err(MdrError::TooShort { len });
        }
        if sectors > MAX_SECTORS {
            return Err(MdrError::TooLong { len });
        }
        let data_len = sectors * SECTOR_LEN;
        Ok(Self {
            data: bytes[..data_len].to_vec(),
            write_protected: tail == 1 && bytes[data_len] != 0,
        })
    }

    /// An unformatted cartridge of `sectors` sectors, every byte `$FF`, as
    /// Fuse makes a new one. The count is clamped to 10..=254.
    #[must_use]
    pub fn blank(sectors: usize) -> Self {
        let sectors = sectors.clamp(MIN_SECTORS, MAX_SECTORS);
        Self {
            data: vec![0xFF; sectors * SECTOR_LEN],
            write_protected: false,
        }
    }

    /// The number of sectors on the tape.
    #[must_use]
    pub fn sector_count(&self) -> usize {
        self.data.len() / SECTOR_LEN
    }

    /// Every sector, in tape order, without the write-protect byte.
    #[must_use]
    pub fn data(&self) -> &[u8] {
        &self.data
    }

    /// Mutable access to the sectors. Their length cannot change.
    #[must_use]
    pub fn data_mut(&mut self) -> &mut [u8] {
        &mut self.data
    }

    /// One sector's 543 bytes, or `None` past the end of the tape.
    #[must_use]
    pub fn sector(&self, index: usize) -> Option<&[u8]> {
        self.data.get(index * SECTOR_LEN..(index + 1) * SECTOR_LEN)
    }

    /// One sector's header, or `None` past the end of the tape.
    #[must_use]
    pub fn sector_header(&self, index: usize) -> Option<SectorHeader> {
        let sector = self.sector(index)?;
        let mut name = [0; 10];
        name.copy_from_slice(&sector[4..14]);
        Some(SectorHeader {
            flag: sector[0],
            number: sector[1],
            name,
            checksum_ok: checksum(&sector[..HEADER_LEN - 1]) == sector[HEADER_LEN - 1],
        })
    }

    /// Writes the image as an `.mdr` file, with the write-protect byte.
    #[must_use]
    pub fn to_bytes(&self) -> Vec<u8> {
        let mut bytes = Vec::with_capacity(self.data.len() + 1);
        bytes.extend_from_slice(&self.data);
        bytes.push(u8::from(self.write_protected));
        bytes
    }
}

/// The Interface 1 ROM's checksum: an 8-bit sum with end-around carry
/// that never yields `$FF`.
///
/// The ROM loop is `ADD A,(HL)`, `ADC A,1`, then `DEC A` unless the
/// `ADC` gave zero.
#[must_use]
pub fn checksum(bytes: &[u8]) -> u8 {
    let mut sum: u16 = 0;
    for &byte in bytes {
        sum += u16::from(byte);
        let carry = sum >> 8;
        sum &= 0xFF;
        sum += carry + 1;
        if sum == 0x100 {
            sum = 0;
        } else {
            sum -= 1;
        }
    }
    sum as u8
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A formatted sector numbered `number` on a cartridge called `name`.
    fn formatted_sector(number: u8, name: &[u8; 10]) -> Vec<u8> {
        let mut sector = vec![0xFC; SECTOR_LEN];
        sector[0] = 0x01;
        sector[1] = number;
        sector[2] = 0;
        sector[3] = 0;
        sector[4..14].copy_from_slice(name);
        sector[14] = checksum(&sector[..14]);
        sector
    }

    #[test]
    fn the_sector_is_543_bytes() {
        assert_eq!(SECTOR_LEN, 543);
        assert_eq!(MAX_SECTORS * SECTOR_LEN + 1, 137_923);
    }

    #[test]
    fn a_full_image_parses_with_its_write_protect_byte() {
        let mut bytes = vec![0; MAX_SECTORS * SECTOR_LEN];
        bytes.push(1);
        let image = MdrImage::parse(&bytes).expect("a full image");
        assert_eq!(image.sector_count(), MAX_SECTORS);
        assert!(image.write_protected);
        assert_eq!(image.to_bytes(), bytes);
    }

    #[test]
    fn a_short_tape_and_a_missing_flag_are_allowed() {
        let image = MdrImage::parse(&vec![0; 180 * SECTOR_LEN]).expect("180 sectors");
        assert_eq!(image.sector_count(), 180);
        assert!(!image.write_protected);
    }

    #[test]
    fn bad_lengths_are_rejected() {
        assert_eq!(
            MdrImage::parse(&[0; 9 * SECTOR_LEN]),
            Err(MdrError::TooShort {
                len: 9 * SECTOR_LEN
            })
        );
        assert_eq!(
            MdrImage::parse(&vec![0; 255 * SECTOR_LEN]),
            Err(MdrError::TooLong {
                len: 255 * SECTOR_LEN
            })
        );
        assert_eq!(
            MdrImage::parse(&vec![0; 20 * SECTOR_LEN + 2]),
            Err(MdrError::Misaligned {
                len: 20 * SECTOR_LEN + 2
            })
        );
        assert!(
            MdrError::Misaligned { len: 3 }
                .to_string()
                .contains("543-byte sectors")
        );
    }

    #[test]
    fn a_blank_cartridge_is_unformatted_and_clamped() {
        let image = MdrImage::blank(500);
        assert_eq!(image.sector_count(), MAX_SECTORS);
        assert!(image.data().iter().all(|&b| b == 0xFF));
        assert_eq!(MdrImage::blank(1).sector_count(), MIN_SECTORS);
    }

    #[test]
    fn sector_headers_decode_and_check() {
        let mut bytes = Vec::new();
        for n in (245..=254).rev() {
            bytes.extend(formatted_sector(n, b"GAMES     "));
        }
        let mut image = MdrImage::parse(&bytes).expect("ten sectors");
        let header = image.sector_header(0).expect("sector 0");
        assert_eq!(header.number, 254);
        assert_eq!(&header.name, b"GAMES     ");
        assert!(header.checksum_ok);
        image.data_mut()[5] ^= 0xFF;
        assert!(!image.sector_header(0).expect("sector 0").checksum_ok);
        assert!(image.sector_header(10).is_none());
    }

    #[test]
    fn the_checksum_never_yields_ff() {
        assert_eq!(checksum(&[]), 0);
        assert_eq!(checksum(&[1, 2, 3]), 6);
        // $FE + $01 would be $FF; the ROM folds it to zero.
        assert_eq!(checksum(&[0xFE, 0x01]), 0);
        // A carry out of bit 7 comes back in at bit 0.
        assert_eq!(checksum(&[0xF0, 0x20]), 0x11);
        for a in 0..=255u8 {
            for b in [0u8, 1, 0x7F, 0xFE, 0xFF] {
                assert_ne!(checksum(&[a, b]), 0xFF);
            }
        }
    }
}
