//! The Acorn Plus 3: a WD1770 disc interface, one or two 3.5" drives and ADFS.
//!
//! The Plus 3 plugs into the Electron's expansion connector and watches the
//! whole bus. It answers in three places:
//!
//! | Address         | Direction | What                                        |
//! |-----------------|-----------|---------------------------------------------|
//! | `&FCC0`         | write     | drive latch (reads `&FF`)                   |
//! | `&FCC4-&FCC7`   | both      | WD1770 status/command, track, sector, data  |
//! | `&8000-&BFFF`   | read      | the ADFS ROM, while its ROM latch holds 4   |
//!
//! The drive latch at `&FCC0`:
//!
//! | Bit | Meaning                                                   |
//! |-----|-----------------------------------------------------------|
//! | 0   | select drive 0                                            |
//! | 1   | select drive 1                                            |
//! | 2   | side (1 = side 1)                                         |
//! | 3   | density (1 = single, FM; 0 = double, MFM)                 |
//! | 5   | `MR`, active low: 0 holds the 1770 in reset               |
//!
//! The Plus 3 keeps its own copy of the paged-ROM number: it latches every
//! write to the ULA's `&FE05` whose top four bits are clear, and drives the
//! ADFS ROM onto the bus when that number is 4. The ULA drives its own pages
//! (keyboard and BASIC) at the same time, and the two are wired together, so a
//! read sees the AND of both. The OS selects ROM 4 through ROM 12, as the
//! Electron needs, so the ULA has let go by then.
//!
//! **The 1770's `INTRQ` and `DRQ` are not connected to the CPU.** ADFS polls
//! the status register for every byte (its transfer loops at `&BA38-&BA60`
//! rotate the busy and DRQ bits into carry). The expansion bus does carry NMI,
//! which the Advanced User Guide reserves for disc and Econet interfaces, but
//! the Plus 3 leaves it alone.
//!
//! The 1770 is clocked at 8 MHz, half the 16 MHz the expansion connector
//! supplies. The controller model's timing is a relaxed countdown in host
//! cycles rather than a bit-cell model; it is ticked once per 2 MHz master
//! tick.
//!
//! # Sources
//!
//! No Plus 3 manual is in the reference library, so the facts above come from
//! two reference emulators, read for facts only (see
//! `198x/emulators/acorn-electron/INDEX.md`): MAME
//! `src/devices/bus/electron/plus3.cpp` at `84981cc1` (latch bits, the `&FE05`
//! snoop, bank 4, the 8 MHz clock, the unwired interrupts) and Elkulator 1.0
//! `src/1770.c` (the same latch and registers, every NMI commented out). The
//! `&FE05` page rule is from the Electron reference's ROM paging section.

use serde::{Deserialize, Serialize};
use western_digital_wd1770::{Disk, Wd1770};

/// The paged-ROM number the ADFS ROM answers to.
pub const ADFS_ROM_PAGE: u8 = 4;
/// Drives the Plus 3 can address.
pub const DRIVES: usize = 2;

/// Latch bit: select drive 0.
const LATCH_DRIVE_0: u8 = 0x01;
/// Latch bit: select drive 1.
const LATCH_DRIVE_1: u8 = 0x02;
/// Latch bit: side 1.
const LATCH_SIDE: u8 = 0x04;
/// Latch bit: single density.
const LATCH_SINGLE_DENSITY: u8 = 0x08;
/// Latch bit: `MR`, active low.
const LATCH_NOT_RESET: u8 = 0x20;

/// The controller drive index used while the latch selects no drive. The
/// WD1770 model has four drive positions and the Plus 3 wires two, so this one
/// is always empty.
const NO_DRIVE: usize = 3;

/// The Plus 3 expansion: its WD1770, its drive latch, its ROM latch and the
/// ADFS ROM.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Plus3 {
    fdc: Wd1770,
    /// The last byte written to `&FCC0`.
    latch: u8,
    /// The paged-ROM number the Plus 3 has latched from `&FE05`.
    rom_latch: u8,
    /// The 16 KB ADFS ROM, or empty when none is fitted.
    adfs: Vec<u8>,
}

impl Plus3 {
    /// A Plus 3 with the given ADFS ROM (empty for none) and no discs.
    ///
    /// The latch powers up with `MR` released and no drive selected, as MAME's
    /// device does until the first write.
    #[must_use]
    pub fn new(adfs: Vec<u8>) -> Self {
        let mut fdc = Wd1770::new();
        fdc.set_drive(NO_DRIVE);
        Self {
            fdc,
            latch: LATCH_NOT_RESET,
            rom_latch: 0,
            adfs,
        }
    }

    /// The controller.
    #[must_use]
    pub fn fdc(&self) -> &Wd1770 {
        &self.fdc
    }

    /// The drive latch, as last written to `&FCC0`.
    #[must_use]
    pub fn latch(&self) -> u8 {
        self.latch
    }

    /// The paged-ROM number the Plus 3 has latched.
    #[must_use]
    pub fn rom_latch(&self) -> u8 {
        self.rom_latch
    }

    /// Whether an ADFS ROM is fitted.
    #[must_use]
    pub fn has_adfs(&self) -> bool {
        !self.adfs.is_empty()
    }

    /// The drive the latch selects, if any.
    #[must_use]
    pub fn selected_drive(&self) -> Option<usize> {
        // Drive 1 wins when both bits are set, as in MAME's decode.
        if self.latch & LATCH_DRIVE_1 != 0 {
            Some(1)
        } else if self.latch & LATCH_DRIVE_0 != 0 {
            Some(0)
        } else {
            None
        }
    }

    /// The side the latch selects.
    #[must_use]
    pub fn side(&self) -> u8 {
        u8::from(self.latch & LATCH_SIDE != 0)
    }

    /// Whether the latch asks for double density (MFM).
    #[must_use]
    pub fn double_density(&self) -> bool {
        self.latch & LATCH_SINGLE_DENSITY == 0
    }

    /// Whether the latch holds the 1770 in reset.
    #[must_use]
    pub fn in_reset(&self) -> bool {
        self.latch & LATCH_NOT_RESET == 0
    }

    /// Put a disc in a drive (0 or 1). A higher number is ignored.
    pub fn insert_disk(&mut self, drive: usize, disk: Disk) {
        if drive < DRIVES {
            self.fdc.insert_disk(drive, disk);
        }
    }

    /// Take the disc out of a drive.
    pub fn eject_disk(&mut self, drive: usize) -> Option<Disk> {
        if drive < DRIVES {
            self.fdc.eject_disk(drive)
        } else {
            None
        }
    }

    /// The disc in a drive.
    #[must_use]
    pub fn disk(&self, drive: usize) -> Option<&Disk> {
        if drive < DRIVES {
            self.fdc.disk(drive)
        } else {
            None
        }
    }

    /// The byte the Plus 3 drives in the paged-ROM window, or `&FF` when it
    /// is not driving.
    #[must_use]
    pub fn rom_byte(&self, addr: u16) -> u8 {
        if self.rom_latch == ADFS_ROM_PAGE {
            self.adfs
                .get(usize::from(addr & 0x3FFF))
                .copied()
                .unwrap_or(0xFF)
        } else {
            0xFF
        }
    }

    /// A read in the `&FC00-&FDFF` I/O pages. Returns `None` where the Plus 3
    /// does not answer.
    pub fn io_read(&mut self, addr: u16) -> Option<u8> {
        match addr {
            // The latch is write-only; the data bus floats high.
            0xFCC0 => Some(0xFF),
            0xFCC4..=0xFCC7 => Some(self.fdc.read((addr & 0x03) as u8)),
            _ => None,
        }
    }

    /// The same read without side effects, for the debugger.
    #[must_use]
    pub fn io_peek(&self, addr: u16) -> Option<u8> {
        match addr {
            0xFCC0 => Some(0xFF),
            0xFCC4 => Some(self.fdc.peek_status()),
            0xFCC5 => Some(self.fdc.track_register()),
            // The sector and data registers have no side-effect-free view.
            0xFCC6 | 0xFCC7 => Some(0xFF),
            _ => None,
        }
    }

    /// A write anywhere on the bus. The Plus 3 decodes its own registers and
    /// snoops the ULA's `&FE05`.
    pub fn write(&mut self, addr: u16, value: u8) {
        match addr {
            0xFCC0 => self.write_latch(value),
            0xFCC4..=0xFCC7 => {
                // While MR is low the 1770 ignores its registers.
                if !self.in_reset() {
                    self.fdc.write((addr & 0x03) as u8, value);
                }
            }
            0xFE05 if value & 0xF0 == 0 => self.rom_latch = value & 0x0F,
            _ => {}
        }
    }

    fn write_latch(&mut self, value: u8) {
        let was_reset = self.in_reset();
        self.latch = value;
        self.fdc
            .set_drive(self.selected_drive().unwrap_or(NO_DRIVE));
        self.fdc.set_side(self.side());
        if self.in_reset() && !was_reset {
            self.fdc.master_reset();
        }
    }

    /// Advance the controller one host cycle.
    pub fn tick(&mut self) {
        self.fdc.tick();
    }
}

/// Build a drive's disc from an ADFS `.adf` or `.adl` image.
///
/// The format comes from the image's free-space map, or from its size when
/// the map is unreadable; a trimmed image is padded to its full size so every
/// sector can be written. Sectors are numbered from 0, sixteen 256-byte
/// sectors to a track, and a two-sided image interleaves its sides track by
/// track.
///
/// # Errors
///
/// Returns the format crate's error for an empty or oversized image.
pub fn adfs_disk(image: &[u8], writable: bool) -> Result<Disk, format_acorn_adfs::Error> {
    let format = format_acorn_adfs::detect(image)?;
    Ok(Disk::new(
        format_acorn_adfs::padded(image, format),
        format.tracks(),
        format.sides(),
        format_acorn_adfs::SECTORS_PER_TRACK,
        format_acorn_adfs::SECTOR_SIZE,
    )
    .with_first_sector_id(0)
    .write_protected(!writable))
}

#[cfg(test)]
mod tests {
    use super::*;
    use format_acorn_adfs::Format;

    fn settle(p: &mut Plus3) {
        for _ in 0..1000 {
            p.tick();
        }
    }

    #[test]
    fn the_latch_selects_drive_side_and_density() {
        let mut p = Plus3::new(Vec::new());
        assert_eq!(p.selected_drive(), None);
        p.write(0xFCC0, 0x21);
        assert_eq!(p.selected_drive(), Some(0));
        assert_eq!(p.side(), 0);
        assert!(p.double_density());
        p.write(0xFCC0, 0x2E);
        assert_eq!(p.selected_drive(), Some(1));
        assert_eq!(p.side(), 1);
        assert!(!p.double_density());
        assert_eq!(p.io_read(0xFCC0), Some(0xFF), "the latch is write-only");
        assert_eq!(p.io_read(0xFCC1), None, "only &FCC0 and &FCC4-7 decode");
    }

    #[test]
    fn the_1770_registers_sit_at_fcc4_to_fcc7() {
        let mut p = Plus3::new(Vec::new());
        p.write(0xFCC5, 0x5A);
        p.write(0xFCC6, 0x07);
        assert_eq!(p.io_read(0xFCC5), Some(0x5A));
        assert_eq!(p.io_read(0xFCC6), Some(0x07));
        assert_eq!(p.io_peek(0xFCC5), Some(0x5A));
    }

    #[test]
    fn mr_low_resets_the_1770_and_locks_its_registers() {
        let mut p = Plus3::new(Vec::new());
        p.write(0xFCC6, 0x09);
        p.write(0xFCC0, 0x00); // MR low
        assert!(p.in_reset());
        assert_eq!(
            p.io_read(0xFCC6),
            Some(1),
            "MR returns the sector register to 1"
        );
        p.write(0xFCC6, 0x05);
        assert_eq!(
            p.io_read(0xFCC6),
            Some(1),
            "writes are ignored during reset"
        );
        p.write(0xFCC0, 0x20); // released
        p.write(0xFCC6, 0x05);
        assert_eq!(p.io_read(0xFCC6), Some(5));
    }

    #[test]
    fn the_rom_latch_follows_fe05_writes_with_a_clear_top_nibble() {
        let mut p = Plus3::new(vec![0xAB; 0x4000]);
        assert_eq!(p.rom_byte(0x8000), 0xFF);
        p.write(0xFE05, 0x04);
        assert_eq!(p.rom_latch(), ADFS_ROM_PAGE);
        assert_eq!(p.rom_byte(0x8000), 0xAB);
        // An interrupt clear carries a set top nibble; the latch keeps 4.
        p.write(0xFE05, 0x30);
        assert_eq!(p.rom_byte(0xBFFF), 0xAB);
        p.write(0xFE05, 0x0A);
        assert_eq!(p.rom_byte(0x8000), 0xFF, "BASIC's page releases the bus");
    }

    #[test]
    fn a_sector_reads_through_the_latch_and_registers() {
        let mut image = format_acorn_adfs::blank(Format::L, "T");
        // Stamp track 3, side 1, sector 9: an .adl interleaves the sides.
        let off = ((3 * 2 + 1) * 16 + 9) * 256;
        image[off..off + 4].copy_from_slice(b"ELK3");
        let mut p = Plus3::new(Vec::new());
        p.insert_disk(1, adfs_disk(&image, true).expect("disc"));

        p.write(0xFCC0, 0x20 | LATCH_DRIVE_1 | LATCH_SIDE);
        p.write(0xFCC7, 3); // data = target track
        p.write(0xFCC4, 0x10); // Seek
        settle(&mut p);
        assert_eq!(p.io_read(0xFCC5), Some(3));
        p.write(0xFCC6, 9);
        p.write(0xFCC4, 0x80); // Read Sector
        let mut bytes = Vec::new();
        for _ in 0..10_000 {
            p.tick();
            let status = p.io_read(0xFCC4).unwrap_or(0);
            if status & 0x02 != 0 {
                bytes.push(p.io_read(0xFCC7).unwrap_or(0));
            } else if status & 0x01 == 0 && !bytes.is_empty() {
                break;
            }
        }
        assert_eq!(bytes.len(), 256, "one whole sector, polled");
        assert_eq!(&bytes[..4], b"ELK3");
    }

    #[test]
    fn no_drive_selected_finds_no_sector() {
        let mut p = Plus3::new(Vec::new());
        p.insert_disk(
            0,
            adfs_disk(&format_acorn_adfs::blank(Format::M, "T"), true).expect("disc"),
        );
        p.write(0xFCC0, 0x20); // MR released, no drive
        p.write(0xFCC6, 0);
        p.write(0xFCC4, 0x80);
        settle(&mut p);
        let status = p.io_read(0xFCC4).unwrap_or(0);
        assert_ne!(status & 0x10, 0, "record not found: {status:02X}");
    }

    #[test]
    fn adfs_disk_pads_and_protects() {
        let disk = adfs_disk(&[0u8; 300], false).expect("disc");
        assert_eq!(disk.data().len(), Format::S.image_len());
        assert!(disk.is_write_protected());
        assert_eq!((disk.tracks(), disk.sides()), (40, 1));
        assert!(adfs_disk(&[], true).is_err());
    }
}
