//! EasyFlash cartridge (CRT hardware type 32).
//!
//! Two AM29F040B flash chips (512 KiB each: 64 × 8 KiB banks for ROML and
//! ROMH), a 6-bit bank register at `$DE00`, a mode register at `$DE02`
//! (GAME/EXROM line control + LED), and 256 bytes of RAM at `$DF00-$DFFF`.
//! Ported against VICE's `easyflash.c`.
//!
//! At reset both registers clear; with the boot jumper in its default
//! position that puts the cartridge in Ultimax, so the menu in ROMH bank 0
//! owns the `$E000` vectors and runs before the KERNAL.

use serde::{Deserialize, Serialize};

use crate::flash040::Flash040;

/// GAME/EXROM line states per `(jumper, mode-register low bits)` — VICE's
/// `easyflash_memconfig`, re-expressed as line levels. Index is
/// `jumper<<3 | (reg02 & 7)`; the value packs our `Cartridge`-style flags
/// `(exrom_asserted, game_asserted)` — asserted = line pulled low.
///
/// VICE encodes the same table as CMODE values 0-3; the mapping is
/// 0 = 8K GAME (exrom asserted), 1 = 16K (both), 2 = off (neither),
/// 3 = Ultimax (game only).
const MEMCONFIG: [(bool, bool); 16] = [
    // jumper off, mode 0: GAME line follows the jumper (asserted).
    (false, true), // 3: ultimax
    (false, true), // 3: reserved
    (true, true),  // 1: 16k
    (true, true),  // 1: reserved
    // jumper off, mode 1: lines follow the register bits.
    (false, false), // 2: off
    (false, true),  // 3: ultimax
    (true, false),  // 0: 8k
    (true, true),   // 1: 16k
    // jumper on, mode 0: GAME line follows the jumper (deasserted).
    (false, false), // 2: off
    (false, true),  // 3: ultimax (game bit forces the line)
    (true, false),  // 0: 8k
    (true, true),   // 1: 16k
    // jumper on, mode 1: lines follow the register bits.
    (false, false), // 2: off
    (false, true),  // 3: ultimax
    (true, false),  // 0: 8k
    (true, true),   // 1: 16k
];

#[derive(Clone, Debug, Serialize, Deserialize)]
pub(crate) struct EasyFlash {
    low: Flash040,
    high: Flash040,
    /// `$DE00`: 6-bit bank register.
    bank: u8,
    /// `$DE02`: LED (bit 7), mode (bit 2), EXROM (bit 1), GAME (bit 0).
    control: u8,
    /// 256 bytes of cartridge RAM at `$DF00-$DFFF`. Powers up non-zero —
    /// EasyFlash software relies on it (VICE bug #469); `$FF` here.
    ram: Vec<u8>,
    /// Boot jumper. `false` (the shipping default) forces Ultimax while the
    /// mode register's bit 2 is clear, so the flash menu boots.
    jumper: bool,
}

impl EasyFlash {
    /// Builds the cartridge from the two 512 KiB chip images.
    pub(crate) fn new(low: Vec<u8>, high: Vec<u8>) -> Self {
        Self {
            low: Flash040::new(low),
            high: Flash040::new(high),
            bank: 0,
            control: 0,
            ram: vec![0xFF; 0x100],
            jumper: false,
        }
    }

    /// `(exrom_asserted, game_asserted)` for the current register state.
    #[must_use]
    pub(crate) fn lines(&self) -> (bool, bool) {
        MEMCONFIG[usize::from(u8::from(self.jumper) << 3 | (self.control & 0x07))]
    }

    /// Ultimax mode: GAME asserted, EXROM not.
    #[must_use]
    pub(crate) fn ultimax(&self) -> bool {
        let (exrom, game) = self.lines();
        game && !exrom
    }

    fn flash_addr(&self, addr: u16) -> u32 {
        (u32::from(self.bank) << 13) | u32::from(addr & 0x1FFF)
    }

    pub(crate) fn roml_read(&self, addr: u16) -> u8 {
        self.low.read(self.flash_addr(addr))
    }

    pub(crate) fn roml_store(&mut self, addr: u16, value: u8) {
        self.low.store(self.flash_addr(addr), value);
    }

    pub(crate) fn romh_read(&self, addr: u16) -> u8 {
        self.high.read(self.flash_addr(addr))
    }

    pub(crate) fn romh_store(&mut self, addr: u16, value: u8) {
        self.high.store(self.flash_addr(addr), value);
    }

    /// `$DE00-$DEFF` write: even addresses hit the bank register, odd the
    /// mode register (decoded on address bit 1, matching the hardware).
    pub(crate) fn io1_write(&mut self, addr: u16, value: u8) {
        if addr & 2 == 0 {
            self.bank = value & 0x3F;
        } else {
            // Only LED, mode, EXROM, GAME are latched.
            self.control = value & 0x87;
        }
    }

    /// `$DF00-$DFFF`: cartridge RAM.
    #[must_use]
    pub(crate) fn io2_read(&self, addr: u16) -> u8 {
        self.ram[usize::from(addr & 0xFF)]
    }

    pub(crate) fn io2_write(&mut self, addr: u16, value: u8) {
        self.ram[usize::from(addr & 0xFF)] = value;
    }

    /// `(bank, control)` register pair, for debug surfaces.
    #[must_use]
    pub(crate) fn registers(&self) -> (u8, u8) {
        (self.bank, self.control)
    }

    /// One phi2 cycle for the erase state machines.
    pub(crate) fn tick(&mut self) {
        self.low.tick();
        self.high.tick();
    }

    /// ROMH byte for the VIC's Ultimax fetch window (no flash-state side
    /// effects — the VIC always sees the array).
    #[must_use]
    pub(crate) fn romh_peek(&self, addr: u16) -> u8 {
        self.high.data()[self.flash_addr(addr) as usize]
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn cart() -> EasyFlash {
        let mut low = vec![0xFF; crate::flash040::FLASH_SIZE];
        let mut high = vec![0xFF; crate::flash040::FLASH_SIZE];
        low[0] = 0x11; // bank 0 ROML byte 0
        low[0x2000] = 0x22; // bank 1 ROML byte 0
        high[0x1FFC] = 0x34; // bank 0 ROMH $FFFC (reset vector low)
        high[0x1FFD] = 0x12;
        EasyFlash::new(low, high)
    }

    #[test]
    fn boots_in_ultimax_with_menu_vectors() {
        let ef = cart();
        assert!(ef.ultimax());
        assert_eq!(ef.romh_read(0xFFFC), 0x34);
        assert_eq!(ef.romh_read(0xFFFD), 0x12);
    }

    #[test]
    fn bank_register_selects_flash_banks() {
        let mut ef = cart();
        assert_eq!(ef.roml_read(0x8000), 0x11);
        ef.io1_write(0xDE00, 0x01);
        assert_eq!(ef.roml_read(0x8000), 0x22);
    }

    #[test]
    fn mode_register_drives_game_exrom_lines() {
        let mut ef = cart();
        // Mode bit set + both lines released → cartridge off.
        ef.io1_write(0xDE02, 0x04);
        assert_eq!(ef.lines(), (false, false));
        // 16K: GAME + EXROM asserted.
        ef.io1_write(0xDE02, 0x07);
        assert_eq!(ef.lines(), (true, true));
        // 8K: EXROM only.
        ef.io1_write(0xDE02, 0x06);
        assert_eq!(ef.lines(), (true, false));
        // Boot mode again (mode bit clear, jumper default): ultimax.
        ef.io1_write(0xDE02, 0x00);
        assert!(ef.ultimax());
    }

    #[test]
    fn io2_ram_powers_up_nonzero_and_stores() {
        let mut ef = cart();
        assert_eq!(ef.io2_read(0xDF10), 0xFF);
        ef.io2_write(0xDF10, 0x42);
        assert_eq!(ef.io2_read(0xDF10), 0x42);
    }

    #[test]
    fn flash_program_through_roml_window() {
        let mut ef = cart();
        // The chip decodes the low 11 bits of the *flash* address; with
        // bank 0 selected the unlock addresses sit inside the ROML window.
        ef.roml_store(0x8555, 0xAA);
        ef.roml_store(0x82AA, 0x55);
        ef.roml_store(0x8555, 0xA0);
        ef.roml_store(0x9000, 0x5A);
        assert_eq!(ef.roml_read(0x9000), 0x5A);
    }
}
