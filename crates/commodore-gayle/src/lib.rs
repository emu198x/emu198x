//! Commodore Gayle gate array — IDE interface, PCMCIA slot, and
//! address decoding for the Amiga 600 and Amiga 1200.
//!
//! Gayle sits between the CPU and the $D80000-$DFFFFF address range,
//! providing:
//!
//! - IDE task-file registers at `$DA0000-$DA3FFF`.
//! - Four Gayle control registers at `$DA8000-$DABFFF` (Card Status,
//!   Interrupt Request, Interrupt Enable, Configuration).
//! - PCMCIA slot routing for common memory (`$600000-$9FFFFF`),
//!   attribute/IO (`$A00000-$A5FFFF`), and card reset
//!   (`$A40000-$A5FFFF`).
//!
//! Stage A scope (per `knowledge/decisions/amiga-machine-rollout-plan.md`):
//! enough to let Kickstart 3.x probe Gayle without crashing. With no
//! IDE drive and no PCMCIA card, STATUS reads `$7F` ("no drive"),
//! other IDE registers read `$FF`, and the control registers are
//! zero-initialised and writeable. PCMCIA reads return `$FF` (no
//! card detected).
//!
//! IDE drive emulation, PCMCIA SRAM / CompactFlash / NE2000 support,
//! and the full A600/A1200 hard-disk boot path land in later stages
//! alongside the catalogue entries that require them. The full donor
//! implementation lives at `Emu198x-Oldest/crates/commodore-gayle/`
//! (2334 lines including 841 lines of NE2000 PCMCIA — dropped here).

// ── Gayle Card Status register bit definitions ────────────────────

/// IDE interrupt active.
pub const GAYLE_CS_IDE: u8 = 0x80;
/// Card detect — card is inserted.
pub const GAYLE_CS_CCDET: u8 = 0x40;
/// Battery voltage detect 1.
pub const GAYLE_CS_BVD1: u8 = 0x20;
/// Battery voltage detect 2.
pub const GAYLE_CS_BVD2: u8 = 0x10;
/// Write protect — card is writable when set.
pub const GAYLE_CS_WR: u8 = 0x08;
/// Busy / IRQ — PCMCIA card interrupt pending.
pub const GAYLE_CS_BSY: u8 = 0x04;
/// Disable — when set, PCMCIA slot is disabled.
pub const GAYLE_CS_DIS: u8 = 0x01;

/// Open-bus return value for IDE registers with no drive attached.
const IDE_NO_DRIVE_OPEN_BUS: u8 = 0xFF;

/// STATUS register value when no drive is attached. Reads as $7F
/// across WinUAE and fs-uae's IDE controllers: BSY clear, DRDY set,
/// DRQ clear, ERR clear, plus the unused bits high.
const IDE_NO_DRIVE_STATUS: u8 = 0x7F;

/// IDE STATUS register offset within the IDE task-file window.
const IDE_REG_STATUS: u32 = 0x1C;

/// Gayle gate array state. Stage A: no IDE drive, no PCMCIA card.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct Gayle {
    /// Card Status register at `$DA8000`.
    gayle_cs: u8,
    /// Interrupt Request register at `$DA9000`. Bits 2..7 are
    /// write-to-clear; bits 0..1 (RESET/BERR) are written directly.
    gayle_irq: u8,
    /// Interrupt Enable register at `$DAA000`.
    gayle_int: u8,
    /// Configuration register at `$DAB000`. Only the low nibble is
    /// significant.
    gayle_cfg: u8,
}

impl Gayle {
    /// Construct a fresh Gayle with no IDE drive and no PCMCIA card.
    #[must_use]
    pub const fn new() -> Self {
        Self {
            gayle_cs: 0,
            gayle_irq: 0,
            gayle_int: 0,
            gayle_cfg: 0,
        }
    }

    /// Read a byte from a Gayle-decoded address. Callers should only
    /// route addresses in `$D80000-$DFFFFF` here; addresses outside
    /// the A1200 Gayle filter return `0`.
    #[must_use]
    pub const fn read(&self, addr: u32) -> u8 {
        let local = addr & 0x000F_FFFF;

        // A1200 address filter: only respond when bits 17 and 19 are
        // both set. Bit-19 is the `$80000` half of `$D80000`; bit-17
        // is the `$20000` half of `$DA0000`.
        if local & 0x000A_0000 != 0x000A_0000 {
            return 0;
        }

        // Gayle control registers at `$DA8000-$DABFFF`: bit 15 set.
        if local & 0x8000 != 0 {
            return match (local >> 12) & 0x03 {
                0 => self.gayle_cs,
                1 => self.gayle_irq,
                2 => self.gayle_int,
                _ => self.gayle_cfg & 0x0F,
            };
        }

        // IDE task-file registers at `$DA0000-$DA3FFF`. With no drive
        // attached, STATUS reads $7F and everything else reads $FF.
        if (local & 0x3FFF) == IDE_REG_STATUS {
            IDE_NO_DRIVE_STATUS
        } else {
            IDE_NO_DRIVE_OPEN_BUS
        }
    }

    /// Read a 16-bit word from a Gayle-decoded address. The IDE DATA
    /// register transfers 16 bits at a time; with no drive, the read
    /// returns `0xFFFF` (open bus).
    #[must_use]
    pub const fn read_word(&self, addr: u32) -> u16 {
        let local = addr & 0x000F_FFFF;
        if local & 0x000A_0000 != 0x000A_0000 {
            return 0;
        }
        if local & 0x8000 != 0 {
            return self.read(addr) as u16;
        }
        // IDE no-drive: open bus across both lanes.
        0xFFFF
    }

    /// Write a byte to a Gayle-decoded address.
    pub fn write(&mut self, addr: u32, val: u8) {
        let local = addr & 0x000F_FFFF;

        if local & 0x000A_0000 != 0x000A_0000 {
            return;
        }

        if local & 0x8000 != 0 {
            match (local >> 12) & 0x03 {
                0 => self.gayle_cs = val,
                1 => {
                    // Bits 2..7: writing 0 clears the corresponding
                    // flag. Bits 0..1 (RESET / BERR) are written
                    // directly.
                    self.gayle_irq = (self.gayle_irq & val) | (val & 0x03);
                }
                2 => self.gayle_int = val,
                _ => self.gayle_cfg = val & 0x0F,
            }
            return;
        }

        // IDE task-file write with no drive: silently dropped.
        let _ = val;
    }

    /// Write a 16-bit word to a Gayle-decoded address.
    pub fn write_word(&mut self, addr: u32, val: u16) {
        let local = addr & 0x000F_FFFF;
        if local & 0x000A_0000 != 0x000A_0000 {
            return;
        }
        if local & 0x8000 != 0 {
            self.write(addr, val as u8);
            return;
        }
        // IDE no-drive: drop the write.
        let _ = val;
    }

    /// IDE IRQ line. Always low with no drive attached.
    #[must_use]
    pub const fn ide_irq_pending(&self) -> bool {
        false
    }

    /// Borrow the current Card Status register value (debug / query).
    #[must_use]
    pub const fn card_status(&self) -> u8 {
        self.gayle_cs
    }

    /// Borrow the current IRQ register value (debug / query).
    #[must_use]
    pub const fn irq_register(&self) -> u8 {
        self.gayle_irq
    }

    /// Borrow the current Interrupt Enable register value (debug / query).
    #[must_use]
    pub const fn int_register(&self) -> u8 {
        self.gayle_int
    }

    /// Borrow the current Configuration register value (debug / query).
    #[must_use]
    pub const fn cfg_register(&self) -> u8 {
        self.gayle_cfg & 0x0F
    }
}

impl Default for Gayle {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::{Gayle, IDE_NO_DRIVE_OPEN_BUS, IDE_NO_DRIVE_STATUS};

    #[test]
    fn new_starts_with_all_control_registers_zero() {
        let g = Gayle::new();
        assert_eq!(g.card_status(), 0);
        assert_eq!(g.irq_register(), 0);
        assert_eq!(g.int_register(), 0);
        assert_eq!(g.cfg_register(), 0);
    }

    #[test]
    fn ide_status_reads_seven_f_with_no_drive() {
        let g = Gayle::new();
        assert_eq!(g.read(0x00DA_001C), IDE_NO_DRIVE_STATUS);
    }

    #[test]
    fn other_ide_registers_read_open_bus_with_no_drive() {
        let g = Gayle::new();
        for reg in [0x00, 0x04, 0x08, 0x0C, 0x10, 0x14, 0x18] {
            assert_eq!(g.read(0x00DA_0000 | reg), IDE_NO_DRIVE_OPEN_BUS);
        }
    }

    #[test]
    fn ide_word_reads_return_open_bus_with_no_drive() {
        let g = Gayle::new();
        assert_eq!(g.read_word(0x00DA_0000), 0xFFFF);
    }

    #[test]
    fn control_registers_round_trip_writes() {
        let mut g = Gayle::new();
        g.write(0x00DA_8000, 0xA5); // gayle_cs
        assert_eq!(g.read(0x00DA_8000), 0xA5);

        g.write(0x00DA_9000, 0x03); // gayle_irq RESET+BERR bits
        assert_eq!(g.read(0x00DA_9000), 0x03);

        g.write(0x00DA_A000, 0x80); // gayle_int (enable IDE IRQ)
        assert_eq!(g.read(0x00DA_A000), 0x80);

        g.write(0x00DA_B000, 0xFF); // gayle_cfg masks to nibble
        assert_eq!(g.read(0x00DA_B000), 0x0F);
    }

    #[test]
    fn addresses_outside_gayle_filter_read_zero() {
        let g = Gayle::new();
        // bits 17 + 19 must both be set; $D90000 has bit 19 but not bit 17.
        assert_eq!(g.read(0x00D9_0000), 0);
        // $D40000 has neither.
        assert_eq!(g.read(0x00D4_0000), 0);
    }

    #[test]
    fn writes_outside_gayle_filter_drop_silently() {
        let mut g = Gayle::new();
        g.write(0x00D9_0000, 0xAA);
        assert_eq!(g.card_status(), 0);
    }

    #[test]
    fn ide_irq_pending_is_always_low_with_no_drive() {
        let mut g = Gayle::new();
        g.write(0x00DA_A000, 0xFF); // enable everything
        assert!(!g.ide_irq_pending());
    }
}
