//! Final Cartridge III freeze cartridge (CRT hardware type 3).
//!
//! 64 KiB ROM as four 16 KiB banks (III+ carries 256 KiB / sixteen banks);
//! each bank splits into an 8 KiB ROML half (`$8000`) and an 8 KiB ROMH half
//! (`$A000`). A single control register at `$DFFF` drives the EXROM/GAME/NMI
//! lines and the bank select. Ported against VICE's `final3.c`.
//!
//! Power-on is 16K mode, bank 0 — the desktop/BASIC menu. Pressing freeze
//! asserts NMI and forces Ultimax (keeping the current bank), so the freezer
//! runs from cartridge ROM. The handler drives NMI and the memory config
//! through `$DFFF`: bit 6 low asserts NMI, bit 6 high releases it; bit 7 high
//! hides the register until the next freeze or reset.

use serde::{Deserialize, Serialize};

const BANK_SIZE: usize = 0x4000; // 16 KiB per bank
const HALF: usize = 0x2000; // 8 KiB ROML / ROMH split

#[derive(Clone, Debug, Serialize, Deserialize)]
pub(crate) struct FinalCartridge3 {
    rom: Vec<u8>,
    /// Number of 16 KiB banks (4 for III, 16 for III+).
    banks: u8,
    /// Last value written to the `$DFFF` register.
    reg: u8,
    /// Register visible. Bit 7 of a `$DFFF` write hides it until a freeze or
    /// reset re-enables it.
    reg_enabled: bool,
    /// EXROM line asserted (pulled low).
    exrom: bool,
    /// GAME line asserted (pulled low).
    game: bool,
    /// Selected 16 KiB bank.
    bank: u8,
    /// Freeze NMI latched.
    nmi: bool,
}

impl FinalCartridge3 {
    /// Builds the cartridge from its ROM image (64 KiB or 256 KiB). Power-on
    /// config is 16K GAME, bank 0 (VICE `final_v3_config_init`).
    pub(crate) fn new(rom: Vec<u8>) -> Self {
        let banks = u8::try_from((rom.len() / BANK_SIZE).clamp(1, 16)).unwrap_or(4);
        Self {
            rom,
            banks,
            reg: 0,
            reg_enabled: true,
            exrom: true, // 16K: both lines asserted
            game: true,
            bank: 0,
            nmi: false,
        }
    }

    fn bank_mask(&self) -> u8 {
        self.banks.saturating_sub(1)
    }

    /// `(exrom_asserted, game_asserted)` for the current state.
    #[must_use]
    pub(crate) fn lines(&self) -> (bool, bool) {
        (self.exrom, self.game)
    }

    /// Ultimax mode: GAME asserted, EXROM not.
    #[must_use]
    pub(crate) fn ultimax(&self) -> bool {
        self.game && !self.exrom
    }

    /// Whether the freeze NMI is currently asserted.
    #[must_use]
    pub(crate) fn nmi_asserted(&self) -> bool {
        self.nmi
    }

    /// Press the freeze button: assert NMI, re-enable the register, and force
    /// Ultimax keeping the current bank (VICE `final_v3_freeze`).
    pub(crate) fn freeze(&mut self) {
        self.reg_enabled = true;
        self.nmi = true;
        self.exrom = false; // Ultimax: EXROM released, GAME asserted
        self.game = true;
    }

    fn bank_base(&self) -> usize {
        usize::from(self.bank) * BANK_SIZE
    }

    pub(crate) fn roml_read(&self, addr: u16) -> u8 {
        self.rom[self.bank_base() + usize::from(addr & 0x1FFF)]
    }

    pub(crate) fn romh_read(&self, addr: u16) -> u8 {
        self.rom[self.bank_base() + HALF + usize::from(addr & 0x1FFF)]
    }

    /// ROMH byte with no side effects (the ROM has none; the VIC fetch path
    /// expects a peek).
    #[must_use]
    pub(crate) fn romh_peek(&self, addr: u16) -> u8 {
        self.romh_read(addr)
    }

    /// `$DE00-$DEFF` read: the top page of the current bank's ROML half mirrors
    /// into I/O-1.
    #[must_use]
    pub(crate) fn io1_read(&self, addr: u16) -> u8 {
        self.rom[self.bank_base() + 0x1E00 + usize::from(addr & 0xFF)]
    }

    /// `$DF00-$DFFF` read: the top page of the current bank's ROML half mirrors
    /// into I/O-2 (so even `$DFFF` reads ROM, not the write-only register).
    #[must_use]
    pub(crate) fn io2_read(&self, addr: u16) -> u8 {
        self.rom[self.bank_base() + 0x1F00 + usize::from(addr & 0xFF)]
    }

    /// `$DFFF` register write. The register decodes only at low byte `$FF`.
    pub(crate) fn io2_write(&mut self, addr: u16, value: u8) {
        if !self.reg_enabled || (addr & 0xFF) != 0xFF {
            return;
        }
        self.reg = value;
        self.reg_enabled = (value & 0x80) == 0; // bit 7 hides the register
        self.nmi = (value & 0x40) == 0; // bit 6 low = NMI asserted
        self.exrom = (value & 0x10) == 0; // bit 4 low = EXROM asserted
        self.game = (value & 0x20) == 0; // bit 5 low = GAME asserted
        self.bank = value & self.bank_mask();
    }

    /// `(register, bank)` for debug surfaces.
    #[must_use]
    pub(crate) fn registers(&self) -> (u8, u8) {
        (self.reg, self.bank)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn cart() -> FinalCartridge3 {
        let mut rom = vec![0u8; 4 * BANK_SIZE];
        rom[0] = 0xA0; // bank 0 ROML byte 0
        rom[HALF] = 0xB0; // bank 0 ROMH byte 0
        rom[HALF + 0x1FFC] = 0x00; // bank 0 $FFFC vector low
        rom[HALF + 0x1FFD] = 0xE0;
        rom[BANK_SIZE] = 0xA1; // bank 1 ROML byte 0
        FinalCartridge3::new(rom)
    }

    #[test]
    fn powers_up_in_16k_bank0() {
        let fc = cart();
        assert_eq!(fc.lines(), (true, true));
        assert!(!fc.ultimax());
        assert!(!fc.nmi_asserted());
        assert_eq!(fc.roml_read(0x8000), 0xA0);
        assert_eq!(fc.romh_read(0xA000), 0xB0);
    }

    #[test]
    fn freeze_asserts_nmi_and_forces_ultimax() {
        let mut fc = cart();
        fc.freeze();
        assert!(fc.nmi_asserted());
        assert!(fc.ultimax());
        // Bank preserved; ROMH now answers the $E000 vectors.
        assert_eq!(fc.romh_read(0xFFFC), 0x00);
        assert_eq!(fc.romh_read(0xFFFD), 0xE0);
    }

    #[test]
    fn register_drives_lines_bank_and_nmi() {
        let mut fc = cart();
        fc.freeze();
        // Release NMI (bit6=1), 16K (bits5,4=0), bank 1, keep register visible.
        fc.io2_write(0xDFFF, 0x40 | 0x01);
        assert!(!fc.nmi_asserted());
        assert_eq!(fc.lines(), (true, true));
        assert_eq!(fc.roml_read(0x8000), 0xA1); // bank 1
    }

    #[test]
    fn bit7_hides_register() {
        let mut fc = cart();
        // Write with bit7 set → register self-hides.
        fc.io2_write(0xDFFF, 0x80 | 0x40);
        assert!(!fc.reg_enabled);
        // Further writes ignored until a freeze re-enables it.
        fc.io2_write(0xDFFF, 0x01);
        assert_eq!(fc.reg, 0xC0);
        fc.freeze();
        fc.io2_write(0xDFFF, 0x41);
        assert_eq!(fc.reg, 0x41);
    }

    #[test]
    fn register_only_decodes_at_dfff() {
        let mut fc = cart();
        let before = fc.reg;
        fc.io2_write(0xDF00, 0xAB); // not $DFFF — ignored
        assert_eq!(fc.reg, before);
    }
}
