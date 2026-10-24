//! A disk as a drive sees it: numbered sectors and a write-protect tab.
//!
//! The drive does not know or care what file the disk came from. Whoever
//! inserts it reads the image and hands over the sectors, so this crate needs
//! no image format of its own.

use serde::{Deserialize, Serialize};

/// The first three sectors of a disk always hold 128 bytes, whatever the
/// density, because the boot ROM reads them before the drive is configured.
const BOOT_SECTORS: usize = 3;
const BOOT_SECTOR_SIZE: usize = 128;

/// A disk in a drive.
///
/// Carries `Serialize`/`Deserialize` because a disk in a drive is live machine
/// state: sectors written during a session are in here and nowhere else, so a
/// save state has to carry it.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Disk {
    sector_size: u16,
    write_protected: bool,
    /// `sector_size` bytes per sector, from sector 1. The first three
    /// sectors of a double-density disk keep their 128 bytes in the low half.
    data: Vec<u8>,
}

impl Disk {
    /// A disk of `sector_size`-byte sectors, 128 or 256.
    ///
    /// `data` holds `sector_size` bytes for every sector, starting with
    /// sector 1. On a 256-byte disk the first three sectors are 128 bytes
    /// long; put each in the low half of its 256-byte place.
    ///
    /// `None` if the sector size is not 128 or 256, if `data` is empty or not
    /// a whole number of sectors, or if it holds more than 65,535 sectors,
    /// which the command frame cannot number.
    #[must_use]
    pub fn new(sector_size: u16, data: Vec<u8>, write_protected: bool) -> Option<Self> {
        let size = usize::from(sector_size);
        let valid = matches!(sector_size, 128 | 256)
            && !data.is_empty()
            && data.len().is_multiple_of(size)
            && data.len() / size <= usize::from(u16::MAX);
        valid.then_some(Self {
            sector_size,
            write_protected,
            data,
        })
    }

    /// Bytes per sector: 128 or 256.
    #[must_use]
    pub fn sector_size(&self) -> u16 {
        self.sector_size
    }

    /// How many sectors the disk holds.
    #[must_use]
    pub fn sector_count(&self) -> u16 {
        u16::try_from(self.data.len() / usize::from(self.sector_size)).unwrap_or(u16::MAX)
    }

    /// Whether the write-protect tab is covered.
    #[must_use]
    pub fn write_protected(&self) -> bool {
        self.write_protected
    }

    /// Every sector's bytes, in the layout [`Self::new`] takes.
    #[must_use]
    pub fn data(&self) -> &[u8] {
        &self.data
    }

    /// The bytes the drive returns for one sector, numbered from 1: 128 for
    /// the first three of a double-density disk, [`Self::sector_size`] for
    /// the rest. `None` past the end of the disk.
    #[must_use]
    pub fn sector(&self, sector: u16) -> Option<&[u8]> {
        let index = usize::from(sector.checked_sub(1)?);
        let size = usize::from(self.sector_size);
        let full = self.data.get(index * size..(index + 1) * size)?;
        if size == 256 && index < BOOT_SECTORS {
            Some(&full[..BOOT_SECTOR_SIZE])
        } else {
            Some(full)
        }
    }

    /// Replace one sector's bytes. `data` must be as long as
    /// [`Self::sector`] says the sector is. Returns whether it was written.
    pub fn write_sector(&mut self, sector: u16, data: &[u8]) -> bool {
        if self.sector(sector).map(<[u8]>::len) != Some(data.len()) {
            return false;
        }
        let start = usize::from(sector - 1) * usize::from(self.sector_size);
        self.data[start..start + data.len()].copy_from_slice(data);
        true
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn only_whole_disks_of_a_real_sector_size_are_accepted() {
        assert!(Disk::new(128, vec![0; 128 * 720], false).is_some());
        assert!(Disk::new(256, vec![0; 256 * 720], false).is_some());
        assert!(Disk::new(512, vec![0; 512], false).is_none());
        assert!(Disk::new(128, vec![], false).is_none());
        assert!(Disk::new(128, vec![0; 200], false).is_none());
        assert!(Disk::new(128, vec![0; 128 * 65_536], false).is_none());
    }

    #[test]
    fn a_double_density_disk_reads_short_boot_sectors() {
        let disk = Disk::new(256, vec![0; 256 * 4], true).expect("valid disk");
        assert_eq!(disk.sector_count(), 4);
        assert!(disk.write_protected());
        assert_eq!(disk.sector(0), None);
        assert_eq!(disk.sector(3).map(<[u8]>::len), Some(128));
        assert_eq!(disk.sector(4).map(<[u8]>::len), Some(256));
        assert_eq!(disk.sector(5), None);
    }

    #[test]
    fn a_write_must_fit_the_sector() {
        let mut disk = Disk::new(256, vec![0; 256 * 4], false).expect("valid disk");
        assert!(!disk.write_sector(1, &[1; 256]));
        assert!(!disk.write_sector(5, &[1; 256]));
        assert!(disk.write_sector(1, &[1; 128]));
        assert!(disk.write_sector(4, &[2; 256]));
        assert_eq!(&disk.data()[..128], &[1; 128][..]);
        assert_eq!(&disk.data()[128..256], &[0; 128][..]);
        assert_eq!(disk.sector(4), Some(&[2; 256][..]));
    }
}
