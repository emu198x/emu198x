//! The floppy and hard-disk DMA chip.
//!
//! The ST's DMA chip sits between the CPU, RAM and the two disk controllers:
//! the WD1772 floppy controller and the ACSI hard-disk port. It has two
//! registers. The DMA address counter lives in the MMU
//! (`atari-st-glue-mmu`); the machine moves the bytes.
//!
//! **`$FF8606` mode (write).** Bit 8 sets the direction (1 = RAM to disk).
//! Bit 7 selects the floppy (1) or the hard disk (0) for transfers. Bit 4
//! makes `$FF8604` the sector count instead of a controller register. Bit 3
//! selects the hard disk's register file instead of the floppy's. Bits 2–1
//! are the floppy controller's A1/A0. Toggling bit 8 resets the chip: its
//! FIFO empties and its sector count clears.
//!
//! **`$FF8606` status (read).** Bit 0 is set when no DMA error occurred,
//! bit 1 when the sector count is not zero, bit 2 the floppy controller's
//! DRQ. The other bits read back from the last `$FF8604` access, as a real
//! STF does.
//!
//! **`$FF8604`.** With mode bit 4 set, a write loads the sector count (in
//! 512-byte units); it cannot be read back, and a read returns the last value
//! seen at `$FF8604`. Otherwise it reaches the selected controller register.
//!
//! The chip moves data through a 16-byte FIFO: from the floppy it collects
//! sixteen bytes, then writes them to RAM in one burst; towards the floppy it
//! fetches sixteen bytes in one burst. A burst happens only while the sector
//! count is not zero; bytes the floppy delivers with a zero count are lost.
//!
//! # Not modelled
//!
//! - The CPU cycles a FIFO burst takes from the 68000 (32 cycles per 16
//!   bytes). Bursts are free here.
//! - The second FIFO the chip alternates with when writing to disk.
//! - The ACSI hard-disk port: its registers read as `$FF` and transfers to it
//!   do nothing.

use serde::{Deserialize, Serialize};

pub const FIFO_SIZE: usize = 16;
pub const SECTOR_SIZE: u32 = 512;

/// What the machine should do with a `$FF8604` access.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Target {
    SectorCount,
    /// Floppy controller register 0–3.
    Floppy(u8),
    HardDisk,
}

/// The DMA chip's state.
#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct Dma {
    mode: u16,
    sector_count: u16,
    fifo: Vec<u8>,
    /// Bytes of the current 512-byte sector moved so far.
    sector_bytes: u32,
    /// True once a transfer has failed; the status shows it inverted.
    error: bool,
    /// The last word seen at `$FF8604`.
    recent: u16,
}

impl Dma {
    /// The mode register.
    #[must_use]
    pub fn mode(&self) -> u16 {
        self.mode
    }

    /// Write `$FF8606`.
    pub fn write_mode(&mut self, value: u16) {
        if (value ^ self.mode) & 0x100 != 0 {
            self.fifo.clear();
            self.sector_count = 0;
            self.sector_bytes = 0;
            self.error = false;
        }
        self.mode = value;
    }

    /// Read `$FF8606`, given the floppy controller's DRQ line.
    #[must_use]
    pub fn status(&self, drq: bool) -> u16 {
        let mut status = self.recent & !0x07;
        if !self.error {
            status |= 1;
        }
        if self.sector_count != 0 {
            status |= 2;
        }
        if drq {
            status |= 4;
        }
        status
    }

    /// Where a `$FF8604` access goes.
    #[must_use]
    pub fn target(&self) -> Target {
        if self.mode & 0x10 != 0 {
            Target::SectorCount
        } else if self.mode & 0x08 != 0 {
            Target::HardDisk
        } else {
            Target::Floppy((self.mode >> 1 & 3) as u8)
        }
    }

    /// A `$FF8604` write the machine routed to the sector count.
    pub fn set_sector_count(&mut self, value: u16) {
        self.sector_count = value & 0xFF;
        self.sector_bytes = 0;
    }

    /// Note the word a `$FF8604` access carried, for the status register's
    /// unused bits.
    pub fn note(&mut self, value: u16) {
        self.recent = value;
    }

    /// The last word seen at `$FF8604`.
    #[must_use]
    pub fn recent(&self) -> u16 {
        self.recent
    }

    /// The sector count.
    #[must_use]
    pub fn sector_count(&self) -> u16 {
        self.sector_count
    }

    /// True when transfers serve the floppy controller rather than the
    /// hard-disk port.
    #[must_use]
    pub fn floppy(&self) -> bool {
        self.mode & 0x80 != 0
    }

    /// True when data flows from RAM to the disk.
    #[must_use]
    pub fn writing(&self) -> bool {
        self.mode & 0x100 != 0
    }

    /// A byte the floppy controller read. Returns the FIFO's 16 bytes when
    /// they are due for RAM.
    pub fn push(&mut self, byte: u8) -> Option<Vec<u8>> {
        self.recent = (self.recent & 0xFF00) | u16::from(byte);
        self.fifo.push(byte);
        if self.fifo.len() < FIFO_SIZE {
            return None;
        }
        let burst = std::mem::take(&mut self.fifo);
        if self.sector_count == 0 {
            return None;
        }
        self.count_burst();
        self.recent = u16::from(burst[FIFO_SIZE - 2]) << 8 | u16::from(burst[FIFO_SIZE - 1]);
        Some(burst)
    }

    /// True when the next byte for the floppy needs a burst from RAM first.
    #[must_use]
    pub fn needs_fill(&self) -> bool {
        self.fifo.is_empty() && self.sector_count != 0
    }

    /// A burst of 16 bytes from RAM, towards the floppy.
    pub fn fill(&mut self, burst: &[u8]) {
        self.fifo = burst.iter().rev().copied().collect();
        self.count_burst();
    }

    /// The next byte for the floppy controller, if the FIFO has one.
    pub fn pull(&mut self) -> Option<u8> {
        self.fifo.pop()
    }

    fn count_burst(&mut self) {
        self.sector_bytes += FIFO_SIZE as u32;
        if self.sector_bytes >= SECTOR_SIZE {
            self.sector_bytes = 0;
            self.sector_count -= 1;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn mode_bits_route_ff8604() {
        let mut dma = Dma::default();
        dma.write_mode(0x90);
        assert_eq!(dma.target(), Target::SectorCount);
        dma.write_mode(0x86);
        assert_eq!(dma.target(), Target::Floppy(3));
        assert!(dma.floppy());
        dma.write_mode(0x08);
        assert_eq!(dma.target(), Target::HardDisk);
        assert!(!dma.floppy());
    }

    #[test]
    fn toggling_the_direction_bit_resets_the_chip() {
        let mut dma = Dma::default();
        dma.set_sector_count(2);
        dma.push(1);
        dma.write_mode(0x100);
        assert_eq!(dma.sector_count(), 0);
        assert!(dma.pull().is_none());
        assert!(dma.writing());
    }

    #[test]
    fn reads_burst_sixteen_bytes_and_count_sectors() {
        let mut dma = Dma::default();
        dma.set_sector_count(1);
        let mut bursts = 0;
        for i in 0..512u32 {
            if let Some(burst) = dma.push(i as u8) {
                assert_eq!(burst.len(), 16);
                assert_eq!(burst[0], (i - 15) as u8);
                bursts += 1;
            }
        }
        assert_eq!(bursts, 32);
        assert_eq!(dma.sector_count(), 0);
        assert_eq!(dma.status(false) & 2, 0);
        assert_eq!(dma.push(0), None, "a zero count drops data");
    }

    #[test]
    fn writes_fill_from_ram_and_pull_in_order() {
        let mut dma = Dma::default();
        dma.write_mode(0x100);
        dma.set_sector_count(1);
        assert!(dma.needs_fill());
        let burst: Vec<u8> = (0..16).collect();
        dma.fill(&burst);
        assert_eq!(dma.pull(), Some(0));
        assert_eq!(dma.pull(), Some(1));
        assert!(!dma.needs_fill());
    }

    #[test]
    fn status_shows_no_error_count_and_drq_over_recent_bits() {
        let mut dma = Dma::default();
        dma.note(0xAB00);
        dma.set_sector_count(3);
        assert_eq!(dma.status(true), 0xAB00 | 0x07);
        assert_eq!(dma.recent(), 0xAB00);
        assert_eq!(dma.mode(), 0);
    }
}
