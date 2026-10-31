//! Action Replay Mk4/5/6 freeze cartridge (CRT hardware type 1).
//!
//! 32 KiB ROM (four 8 KiB banks) plus 8 KiB of onboard RAM, a control
//! register at `$DE00`, and a freeze button wired to `/NMI`. Ported against
//! VICE's `actionreplay.c`.
//!
//! Pressing freeze asserts NMI and forces the cartridge into Ultimax with its
//! RAM enabled, so the freezer's NMI handler runs from cartridge ROM/RAM and
//! owns the `$E000` vectors. The handler releases the NMI and picks a normal
//! memory config by writing `$DE00` (bit 6 clears the freeze).

use serde::{Deserialize, Serialize};

const ROM_SIZE: usize = 0x8000; // 4 x 8 KiB banks
const RAM_SIZE: usize = 0x2000; // 8 KiB
const BANK_SIZE: usize = 0x2000;

#[derive(Clone, Debug, Serialize, Deserialize)]
pub(crate) struct ActionReplay {
    rom: Vec<u8>,
    ram: Vec<u8>,
    /// Last value written to the `$DE00` control register.
    reg: u8,
    /// Cartridge enabled. Cleared by writing `$DE00` bit 2, which switches the
    /// cartridge off until the next freeze or reset.
    active: bool,
    /// EXROM line asserted (pulled low).
    exrom: bool,
    /// GAME line asserted (pulled low).
    game: bool,
    /// Cartridge RAM mapped at ROML (`$8000`) and the `$DF00` window.
    export_ram: bool,
    /// Selected 8 KiB ROM/RAM bank (0-3).
    bank: u8,
    /// Freeze NMI latched. Set on a freeze-button press, cleared when the
    /// handler writes `$DE00` bit 6.
    nmi: bool,
}

impl ActionReplay {
    /// Builds the cartridge from its (up to) 32 KiB ROM image. The power-on
    /// config is 8K GAME, bank 0, RAM off (VICE `actionreplay_config_init`).
    pub(crate) fn new(rom: Vec<u8>) -> Self {
        let mut padded = vec![0u8; ROM_SIZE];
        let len = rom.len().min(ROM_SIZE);
        padded[..len].copy_from_slice(&rom[..len]);
        Self {
            rom: padded,
            ram: vec![0u8; RAM_SIZE],
            reg: 0,
            active: true,
            exrom: true, // 8K GAME: EXROM asserted, GAME released
            game: false,
            export_ram: false,
            bank: 0,
            nmi: false,
        }
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

    /// Press the freeze button: assert NMI and force Ultimax + RAM at bank 0,
    /// so the handler runs from the cartridge (VICE `actionreplay_freeze`).
    pub(crate) fn freeze(&mut self) {
        self.nmi = true;
        self.active = true;
        self.bank = 0;
        self.exrom = false; // Ultimax: EXROM released, GAME asserted
        self.game = true;
        self.export_ram = true;
    }

    fn bank_base(&self) -> usize {
        usize::from(self.bank & 3) * BANK_SIZE
    }

    pub(crate) fn roml_read(&self, addr: u16) -> u8 {
        let off = usize::from(addr & 0x1FFF);
        if self.export_ram {
            self.ram[off]
        } else {
            self.rom[self.bank_base() + off]
        }
    }

    pub(crate) fn roml_store(&mut self, addr: u16, value: u8) {
        if self.export_ram {
            self.ram[usize::from(addr & 0x1FFF)] = value;
        }
    }

    /// ROMH read (`$A000` in 16K, `$E000` in Ultimax) — always the ROM bank.
    pub(crate) fn romh_read(&self, addr: u16) -> u8 {
        self.rom[self.bank_base() + usize::from(addr & 0x1FFF)]
    }

    /// ROMH byte with no side effects (identical to [`Self::romh_read`]; the
    /// ROM has none, but the VIC fetch path expects a peek).
    #[must_use]
    pub(crate) fn romh_peek(&self, addr: u16) -> u8 {
        self.romh_read(addr)
    }

    /// `$DE00-$DEFF` register write.
    pub(crate) fn io1_write(&mut self, value: u8) {
        if !self.active {
            return;
        }
        self.reg = value;
        if (value & 0x23) == 0x22 {
            // Broken "RAM + ROML" contention state on real hardware — VICE
            // forces 8K GAME instead. Bank still follows bits 4-3.
            self.exrom = true;
            self.game = false;
        } else {
            let mode = value & 0x03;
            self.exrom = (mode & 0x02) == 0; // asserted for 8K/16K modes
            self.game = (mode & 0x01) != 0; // asserted for 16K/Ultimax modes
        }
        self.bank = (value >> 3) & 0x03;
        self.export_ram = value & 0x20 != 0;
        if value & 0x40 != 0 {
            self.nmi = false; // release the freeze NMI
        }
        if value & 0x04 != 0 {
            self.active = false; // switch the cartridge off
        }
    }

    /// `$DE00-$DEFF` read — invalid on hardware; returns the last register
    /// value (a clean peek, avoiding the bus-contention corruption VICE warns
    /// about; software never reads this).
    #[must_use]
    pub(crate) fn io1_read(&self) -> u8 {
        self.reg
    }

    /// `$DF00-$DFFF`: cartridge RAM when enabled, else the top page of the
    /// current ROM bank.
    #[must_use]
    pub(crate) fn io2_read(&self, addr: u16) -> u8 {
        let page = 0x1F00 + usize::from(addr & 0xFF);
        if self.export_ram {
            self.ram[page]
        } else {
            self.rom[self.bank_base() + page]
        }
    }

    pub(crate) fn io2_write(&mut self, addr: u16, value: u8) {
        if self.export_ram {
            self.ram[0x1F00 + usize::from(addr & 0xFF)] = value;
        }
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

    fn cart() -> ActionReplay {
        let mut rom = vec![0u8; ROM_SIZE];
        rom[0] = 0x11; // bank 0 ROML byte 0
        rom[0x2000] = 0x22; // bank 1 ROML byte 0
        rom[0x1FFC] = 0x34; // bank 0 $FFFC (Ultimax reset vector low)
        rom[0x1FFD] = 0x12;
        ActionReplay::new(rom)
    }

    #[test]
    fn powers_up_in_8k_game_bank0() {
        let ar = cart();
        assert_eq!(ar.lines(), (true, false));
        assert!(!ar.ultimax());
        assert!(!ar.nmi_asserted());
        assert_eq!(ar.roml_read(0x8000), 0x11);
    }

    #[test]
    fn freeze_asserts_nmi_and_forces_ultimax_ram() {
        let mut ar = cart();
        ar.freeze();
        assert!(ar.nmi_asserted());
        assert!(ar.ultimax());
        // RAM now answers ROML; ROMH still exposes the bank-0 vectors.
        ar.roml_store(0x8000, 0x99);
        assert_eq!(ar.roml_read(0x8000), 0x99);
        assert_eq!(ar.romh_read(0xFFFC), 0x34);
    }

    #[test]
    fn register_selects_mode_bank_and_releases_nmi() {
        let mut ar = cart();
        ar.freeze();
        // Handler writes: release NMI (bit6), 8K GAME (mode 0), bank 1.
        ar.io1_write(0x40 | (1 << 3));
        assert!(!ar.nmi_asserted());
        assert_eq!(ar.lines(), (true, false));
        assert!(!ar.export_ram);
        assert_eq!(ar.roml_read(0x8000), 0x22); // bank 1
    }

    #[test]
    fn bit2_disables_cartridge() {
        let mut ar = cart();
        ar.io1_write(0x04); // disable
        assert!(!ar.active);
        // Further writes are ignored while disabled.
        ar.io1_write(0x01);
        assert_eq!(ar.reg, 0x04);
    }

    #[test]
    fn mode_bits_map_to_lines() {
        let mut ar = cart();
        ar.io1_write(0x00); // 8K GAME
        assert_eq!(ar.lines(), (true, false));
        ar.io1_write(0x01); // 16K
        assert_eq!(ar.lines(), (true, true));
        ar.io1_write(0x02); // RAM / cartridge off
        assert_eq!(ar.lines(), (false, false));
        ar.io1_write(0x03); // Ultimax
        assert_eq!(ar.lines(), (false, true));
    }
}
