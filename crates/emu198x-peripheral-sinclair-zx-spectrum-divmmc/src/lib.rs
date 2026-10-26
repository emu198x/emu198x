//! The DivMMC: an 8 KB EEPROM, 128 KB of banked RAM and an SD card
//! slot, paged over the Spectrum ROM by watching the opcode fetches.
//!
//! The EEPROM normally holds esxDOS. That is its authors' firmware, free
//! to use but not to redistribute here, so it is never part of this
//! repository: [`DivMmc::attach`] takes the image from the user.
//!
//! # Memory
//!
//! While paged in, the interface replaces the bottom 16 KB. The upper
//! 8 KB at `$2000` is always one of sixteen RAM banks. The lower 8 KB
//! at `$0000` depends on the control register:
//!
//! | CONMEM | MAPRAM | `$0000-$1FFF` | `$2000-$3FFF` |
//! |---|---|---|---|
//! | 1 | — | EEPROM, writable unless the jumper protects it | bank, writable |
//! | 0 | 1 | RAM bank 3, read-only | bank, read-only if it is bank 3 |
//! | 0 | 0 | EEPROM, read-only | bank, writable |
//!
//! # Ports
//!
//! All three decode the low byte only.
//!
//! | Port | Access | Use |
//! |---|---|---|
//! | `$E3` | write | control: bits 0-5 bank (bits 0-3 used), bit 6 MAPRAM, bit 7 CONMEM |
//! | `$E7` | write | card select, active low: `%10` in bits 1-0 selects the card |
//! | `$EB` | read and write | SPI data to and from the selected card |
//!
//! MAPRAM, once set, stays set until power is removed: a write cannot
//! clear it and a reset keeps it, which lets esxDOS run from RAM bank 3
//! after a reset.
//!
//! # Automapping
//!
//! CONMEM pages the interface in by itself. Otherwise it pages in when
//! the CPU fetches an opcode from one of the entry points, provided the
//! EEPROM is write-protected or MAPRAM is set:
//!
//! - `$0000`, `$0008`, `$0038`, `$0066`, `$04C6` and `$0562` page in
//!   after the fetch, so the opcode there comes from the Spectrum ROM;
//! - `$3D00-$3DFF` pages in before the fetch, so the opcode there comes
//!   from the interface;
//! - `$1FF8-$1FFF` pages out after the fetch.
//!
//! # Sources
//!
//! Fuse's `peripherals/ide/divmmc.c`, `divxxx.c` and `z80/z80_ops.c`
//! (GPL-2, used for facts only) for the ports, the paging table, the
//! traps and the reset rules; the card follows libspectrum's `mmc.c`.

mod sd_card;

pub use sd_card::{SD_BLOCK_BYTES, SD_IMAGE_UNIT, SdCard, SdImageError};

use emu198x_bus_sinclair_zx_spectrum::{MemoryIntercept, Peripheral};

/// The size of the EEPROM and of each RAM bank.
pub const DIVMMC_PAGE_BYTES: usize = 0x2000;

/// The number of 8 KB RAM banks.
pub const DIVMMC_RAM_BANKS: usize = 16;

const CONMEM: u8 = 0x80;
const MAPRAM: u8 = 0x40;

const PORT_CONTROL: u8 = 0xE3;
const PORT_CARD_SELECT: u8 = 0xE7;
const PORT_SPI: u8 = 0xEB;

/// The entry points that page the interface in after their fetch.
const LATE_TRAPS: [u16; 6] = [0x0000, 0x0008, 0x0038, 0x0066, 0x04C6, 0x0562];

/// An EEPROM image that is not 8 KB.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct DivMmcEepromError {
    /// The length that was supplied.
    pub actual: usize,
}

impl std::fmt::Display for DivMmcEepromError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "expected an {DIVMMC_PAGE_BYTES}-byte DivMMC EEPROM image, got {} bytes",
            self.actual
        )
    }
}

impl std::error::Error for DivMmcEepromError {}

/// The interface's state.
#[derive(Clone, Debug, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct DivMmc {
    attached: bool,
    eeprom: Vec<u8>,
    ram: Vec<u8>,
    control: u8,
    /// Whether the CPU is inside the interface's code, by the traps:
    /// whether it *would* be paged in if CONMEM, MAPRAM and the jumper
    /// allowed it.
    automap: bool,
    paged: bool,
    /// The EEPROM write-protect jumper. Automapping needs it on, or
    /// MAPRAM set.
    pub eeprom_write_protect: bool,
    card_selected: bool,
    /// The SD card slot.
    pub card: SdCard,
}

impl Default for DivMmc {
    fn default() -> Self {
        Self::new()
    }
}

impl DivMmc {
    /// An unplugged interface with the jumper protecting the EEPROM and
    /// an empty card slot.
    #[must_use]
    pub fn new() -> Self {
        Self {
            attached: false,
            eeprom: vec![0xFF; DIVMMC_PAGE_BYTES],
            ram: vec![0; DIVMMC_PAGE_BYTES * DIVMMC_RAM_BANKS],
            control: 0,
            automap: false,
            paged: false,
            eeprom_write_protect: true,
            card_selected: false,
            card: SdCard::new(),
        }
    }

    /// Plugs the interface in with its EEPROM image, as from power-on:
    /// RAM cleared and the control register zero.
    ///
    /// # Errors
    ///
    /// Returns an error, and leaves the interface as it was, if the image
    /// is not 8 KB.
    pub fn attach(&mut self, eeprom: &[u8]) -> Result<(), DivMmcEepromError> {
        if eeprom.len() != DIVMMC_PAGE_BYTES {
            return Err(DivMmcEepromError {
                actual: eeprom.len(),
            });
        }
        self.eeprom.copy_from_slice(eeprom);
        self.ram.fill(0);
        self.control = 0;
        self.attached = true;
        self.reset();
        Ok(())
    }

    /// Whether the interface is plugged in.
    #[must_use]
    pub const fn is_attached(&self) -> bool {
        self.attached
    }

    /// Whether the interface is paged in over the bottom 16 KB.
    #[must_use]
    pub const fn is_paged(&self) -> bool {
        self.paged
    }

    /// The control register, as last written with MAPRAM kept.
    #[must_use]
    pub const fn control(&self) -> u8 {
        self.control
    }

    /// The EEPROM, which esxDOS can rewrite while CONMEM is set and the
    /// jumper is off.
    #[must_use]
    pub fn eeprom(&self) -> &[u8] {
        &self.eeprom
    }

    /// One 8 KB RAM bank.
    #[must_use]
    pub fn ram_bank(&self, bank: usize) -> Option<&[u8]> {
        self.ram
            .get(bank * DIVMMC_PAGE_BYTES..(bank + 1) * DIVMMC_PAGE_BYTES)
    }

    /// A reset: paged out and out of the interface's code, with every
    /// control bit but MAPRAM cleared. RAM and the EEPROM keep their
    /// contents and the card is put back in its idle state.
    pub fn reset(&mut self) {
        self.control &= MAPRAM;
        self.automap = false;
        self.card.reset();
        self.refresh();
    }

    fn refresh(&mut self) {
        self.paged = self.attached
            && (self.control & CONMEM != 0
                || (self.eeprom_write_protect || self.control & MAPRAM != 0) && self.automap);
    }

    fn set_automap(&mut self, automap: bool) {
        self.automap = automap;
        self.refresh();
    }

    fn bank(&self) -> usize {
        usize::from(self.control) & (DIVMMC_RAM_BANKS - 1)
    }

    /// Whether a read from this port reaches the interface. Only the SPI
    /// port can be read; the other two are write-only.
    #[must_use]
    pub fn claims_read(&self, port: u16) -> bool {
        self.attached && port as u8 == PORT_SPI
    }
}

impl Peripheral for DivMmc {
    fn claims_port(&self, port: u16) -> bool {
        self.attached && matches!(port as u8, PORT_CONTROL | PORT_CARD_SELECT | PORT_SPI)
    }

    fn read(&mut self, port: u16) -> u8 {
        if port as u8 == PORT_SPI && self.card_selected {
            self.card.read()
        } else {
            0xFF
        }
    }

    fn write(&mut self, port: u16, value: u8) {
        match port as u8 {
            PORT_CONTROL => {
                self.control = value | (self.control & MAPRAM);
                self.refresh();
            }
            PORT_CARD_SELECT => self.card_selected = value & 0x03 == 0x02,
            PORT_SPI if self.card_selected => self.card.write(value),
            _ => {}
        }
    }

    fn on_m1(&mut self, addr: u16) {
        if self.attached && addr & 0xFF00 == 0x3D00 {
            self.set_automap(true);
        }
    }
}

impl MemoryIntercept for DivMmc {
    fn read_memory(&self, addr: u16) -> Option<u8> {
        if !self.paged || addr >= 0x4000 {
            return None;
        }
        let offset = usize::from(addr) & (DIVMMC_PAGE_BYTES - 1);
        let byte = if addr >= 0x2000 {
            self.ram[self.bank() * DIVMMC_PAGE_BYTES + offset]
        } else if self.control & (CONMEM | MAPRAM) == MAPRAM {
            self.ram[3 * DIVMMC_PAGE_BYTES + offset]
        } else {
            self.eeprom[offset]
        };
        Some(byte)
    }

    fn write_memory(&mut self, addr: u16, value: u8) -> bool {
        if !self.paged || addr >= 0x4000 {
            return false;
        }
        let offset = usize::from(addr) & (DIVMMC_PAGE_BYTES - 1);
        let conmem = self.control & CONMEM != 0;
        let mapram = self.control & MAPRAM != 0;
        if addr >= 0x2000 {
            let bank = self.bank();
            if conmem || !mapram || bank != 3 {
                self.ram[bank * DIVMMC_PAGE_BYTES + offset] = value;
            }
        } else if conmem && !self.eeprom_write_protect {
            self.eeprom[offset] = value;
        }
        true
    }

    fn after_m1(&mut self, addr: u16) {
        if !self.attached {
            return;
        }
        if addr & 0xFFF8 == 0x1FF8 {
            self.set_automap(false);
        } else if LATE_TRAPS.contains(&addr) {
            self.set_automap(true);
        }
    }
}

#[cfg(test)]
mod tests;
