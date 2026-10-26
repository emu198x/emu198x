//! The cartridge's tape loop as the drive sees it.
//!
//! A Microdrive cartridge is an endless loop of tape holding up to 254
//! sectors. Each sector is two blocks the Interface 1 ROM writes: a
//! 15-byte sector header, then a record header, 512 bytes of data and a
//! checksum. The drive synthesises the preambles of ten `$00` and two
//! `$FF` bytes before each block and the gaps between them, so a
//! [`MicrodriveTape`] holds only the sectors, in tape order, and the
//! write-protect tab.
//!
//! This is the layout the drive needs, not a file format. Loading and
//! saving `.mdr` files is a format crate's job: a host reads the file,
//! hands the sectors and the tab to [`MicrodriveTape::new`], and writes
//! [`MicrodriveTape::sectors`] back out. The layout follows libspectrum's
//! `microdrive.c` (GPL-2, used for facts only), which is what Fuse reads
//! and writes.

/// Bytes in a sector header, and in a record header.
pub const HEADER_LEN: usize = 15;
/// Bytes of data in a record.
pub const DATA_LEN: usize = 512;
/// Bytes in one sector: the sector header, the record header, the data
/// and the data checksum.
pub const SECTOR_LEN: usize = HEADER_LEN + HEADER_LEN + DATA_LEN + 1;
/// The most sectors a cartridge's tape holds.
pub const MAX_SECTORS: usize = 254;
/// The fewest sectors [`MicrodriveTape::blank`] makes, as Fuse makes a
/// new cartridge.
pub const MIN_SECTORS: usize = 10;

/// The sectors on a cartridge's tape loop, and its write-protect tab.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct MicrodriveTape {
    sectors: Vec<u8>,
    /// Whether the cartridge's write-protect tab is broken off.
    pub write_protected: bool,
}

impl MicrodriveTape {
    /// A tape holding `sectors`, a whole number of [`SECTOR_LEN`]-byte
    /// sectors in tape order. Returns `None` unless there are 1 to
    /// [`MAX_SECTORS`] of them.
    #[must_use]
    pub fn new(sectors: Vec<u8>, write_protected: bool) -> Option<Self> {
        let count = sectors.len() / SECTOR_LEN;
        if !sectors.len().is_multiple_of(SECTOR_LEN) || !(1..=MAX_SECTORS).contains(&count) {
            return None;
        }
        Some(Self {
            sectors,
            write_protected,
        })
    }

    /// An unformatted tape of `sectors` sectors, every byte `$FF`, as
    /// Fuse makes a new cartridge. The count is clamped to
    /// [`MIN_SECTORS`]`..=`[`MAX_SECTORS`].
    #[must_use]
    pub fn blank(sectors: usize) -> Self {
        let sectors = sectors.clamp(MIN_SECTORS, MAX_SECTORS);
        Self {
            sectors: vec![0xFF; sectors * SECTOR_LEN],
            write_protected: false,
        }
    }

    /// The number of sectors on the tape.
    #[must_use]
    pub fn sector_count(&self) -> usize {
        self.sectors.len() / SECTOR_LEN
    }

    /// Every sector, in tape order.
    #[must_use]
    pub fn sectors(&self) -> &[u8] {
        &self.sectors
    }

    /// One sector's [`SECTOR_LEN`] bytes, or `None` past the end of the
    /// tape.
    #[must_use]
    pub fn sector(&self, index: usize) -> Option<&[u8]> {
        self.sectors
            .get(index * SECTOR_LEN..(index + 1) * SECTOR_LEN)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn new_takes_whole_sectors_up_to_the_most_a_tape_holds() {
        assert!(MicrodriveTape::new(vec![0; SECTOR_LEN], false).is_some());
        assert!(MicrodriveTape::new(vec![0; MAX_SECTORS * SECTOR_LEN], true).is_some());
        assert!(MicrodriveTape::new(Vec::new(), false).is_none());
        assert!(MicrodriveTape::new(vec![0; SECTOR_LEN + 1], false).is_none());
        assert!(MicrodriveTape::new(vec![0; (MAX_SECTORS + 1) * SECTOR_LEN], false).is_none());
    }

    #[test]
    fn blank_clamps_its_length_and_fills_with_ff() {
        let tape = MicrodriveTape::blank(1);
        assert_eq!(tape.sector_count(), MIN_SECTORS);
        assert!(tape.sectors().iter().all(|&b| b == 0xFF));
        assert!(!tape.write_protected);
        assert_eq!(MicrodriveTape::blank(1000).sector_count(), MAX_SECTORS);
        assert_eq!(
            tape.sector(MIN_SECTORS - 1).map(<[u8]>::len),
            Some(SECTOR_LEN)
        );
        assert!(tape.sector(MIN_SECTORS).is_none());
    }
}
