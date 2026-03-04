//! Commodore Gayle gate array — IDE interface and address decoding for
//! the Amiga 600 and Amiga 1200.
//!
//! Gayle sits between the CPU and the $D80000-$DFFFFF address range,
//! providing IDE task-file registers and four control/status registers.
//! Without a drive attached, IDE STATUS reads $7F ("no drive") and other
//! task-file registers read $FF — matching WinUAE behaviour.

/// Gayle gate array state.
///
/// Handles IDE task-file registers ($DA0000+) and four Gayle control
/// registers ($DA8000-$DABFFF). Addresses outside these ranges within
/// $D80000-$DFFFFF return 0 (no PCMCIA card).
#[derive(Debug, Clone)]
pub struct Gayle {
    /// Card Status register ($DA8000).
    gayle_cs: u8,
    /// Interrupt Request register ($DA9000). Bits 2-7 are write-to-clear;
    /// bits 0-1 (RESET/BERR) are written directly.
    gayle_irq: u8,
    /// Interrupt Enable register ($DAA000).
    gayle_int: u8,
    /// Configuration register ($DAB000). Only low 4 bits are significant.
    gayle_cfg: u8,
    /// IDE STATUS register value returned when read. $7F when no drive.
    ide_status: u8,
    /// Whether an IDE drive is attached.
    drive_present: bool,
}

impl Gayle {
    /// Create a new Gayle with no IDE drive attached.
    #[must_use]
    pub fn new() -> Self {
        Self {
            gayle_cs: 0,
            gayle_irq: 0,
            gayle_int: 0,
            gayle_cfg: 0,
            ide_status: 0x7F,
            drive_present: false,
        }
    }

    /// Reset all registers to power-on defaults.
    pub fn reset(&mut self) {
        self.gayle_cs = 0;
        self.gayle_irq = 0;
        self.gayle_int = 0;
        self.gayle_cfg = 0;
        self.ide_status = 0x7F;
    }

    /// Read a byte from a Gayle-decoded address.
    ///
    /// The caller should only invoke this for addresses in $D80000-$DFFFFF.
    /// Addresses that don't match the Gayle filter return 0.
    #[must_use]
    pub fn read(&self, addr: u32) -> u8 {
        let local = addr & 0x0F_FFFF;

        // A1200 address filter: only respond when bits 17 and 19 are both set.
        if local & 0xA_0000 != 0xA_0000 {
            return 0;
        }

        // Gayle control registers ($DA8000-$DABFFF): bit 15 set.
        if local & 0x8000 != 0 {
            return match (local >> 12) & 0x03 {
                0 => self.gayle_cs,
                1 => self.gayle_irq,
                2 => self.gayle_int,
                3 => self.gayle_cfg & 0x0F,
                _ => unreachable!(),
            };
        }

        // IDE task-file registers ($DA0000-$DA3FFF).
        self.read_ide(local)
    }

    /// Write a byte to a Gayle-decoded address.
    ///
    /// The caller should only invoke this for addresses in $D80000-$DFFFFF.
    pub fn write(&mut self, addr: u32, val: u8) {
        let local = addr & 0x0F_FFFF;

        // A1200 address filter.
        if local & 0xA_0000 != 0xA_0000 {
            return;
        }

        // Gayle control registers.
        if local & 0x8000 != 0 {
            match (local >> 12) & 0x03 {
                0 => self.gayle_cs = val,
                1 => {
                    // Bits 2-7: writing 0 clears the corresponding flag.
                    // Bits 0-1 (RESET/BERR): written directly.
                    self.gayle_irq = (self.gayle_irq & val) | (val & 0x03);
                }
                2 => self.gayle_int = val,
                3 => self.gayle_cfg = val & 0x0F,
                _ => unreachable!(),
            }
        }

        // IDE task-file writes are ignored when no drive is present.
    }

    /// True when the IDE interrupt line is asserted and enabled.
    #[must_use]
    pub fn ide_irq_pending(&self) -> bool {
        (self.gayle_int & self.gayle_irq & 0x80) != 0
    }

    /// Decode an IDE task-file read. `local` is the address masked to
    /// $0FFFFF and already verified to have bits 17+19 set.
    fn read_ide(&self, local: u32) -> u8 {
        // Strip bits 13 and 5, then shift right 2 to get register index 0-7.
        let stripped = local & !0x2020;
        let reg = (stripped >> 2) & 0x07;

        if !self.drive_present {
            // No drive: STATUS = $7F, all others = $FF.
            if reg == 7 {
                return self.ide_status;
            }
            return 0xFF;
        }

        // With a drive present we'd dispatch to actual IDE state here.
        // For now, return the same "no drive" defaults.
        if reg == 7 {
            self.ide_status
        } else {
            0xFF
        }
    }
}

impl Default for Gayle {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn no_drive_status_returns_7f() {
        let g = Gayle::new();
        // IDE STATUS register: $DA0000 + offset for reg 7.
        // reg 7 = STATUS. After strip & shift: addr $DA001C → stripped $DA001C, >>2 = 7.
        assert_eq!(g.read(0xDA_001C), 0x7F);
    }

    #[test]
    fn no_drive_other_returns_ff() {
        let g = Gayle::new();
        // reg 1 = ERROR: addr $DA0004, stripped $DA0004, >>2 = 1.
        assert_eq!(g.read(0xDA_0004), 0xFF);
    }

    #[test]
    fn gayle_cs_roundtrip() {
        let mut g = Gayle::new();
        g.write(0xDA_8000, 0xA5);
        assert_eq!(g.read(0xDA_8000), 0xA5);
    }

    #[test]
    fn gayle_irq_write_to_clear() {
        let mut g = Gayle::new();
        // Simulate hardware setting IRQ bits (bits 2-7).
        g.gayle_irq = 0xFC;
        // Write $0C: clears bits 4-7 (they were 1 in irq, 0 in val),
        // keeps bits 2-3 (1 in both), sets bits 0-1 to 0.
        g.write(0xDA_9000, 0x0C);
        // Result: (0xFC & 0x0C) | (0x0C & 0x03) = 0x0C | 0x00 = 0x0C
        assert_eq!(g.read(0xDA_9000), 0x0C);
    }

    #[test]
    fn gayle_cfg_4_bits() {
        let mut g = Gayle::new();
        g.write(0xDA_B000, 0xFF);
        assert_eq!(g.read(0xDA_B000), 0x0F);
    }

    #[test]
    fn ide_address_decode() {
        let g = Gayle::new();
        // Verify the &= !0x2020 >> 2 mapping for each register index.
        // reg 0: base $DA0000 → stripped $DA0000 >> 2 & 7 = 0
        assert_eq!(g.read(0xDA_0000), 0xFF); // DATA (not STATUS)
        // reg 7: $DA001C → stripped $DA001C >> 2 & 7 = 7
        assert_eq!(g.read(0xDA_001C), 0x7F); // STATUS
        // reg 6: $DA0018 → stripped $DA0018 >> 2 & 7 = 6
        assert_eq!(g.read(0xDA_0018), 0xFF); // ALTSTAT (no drive)
    }

    #[test]
    fn address_filter_rejects_low_range() {
        let g = Gayle::new();
        // $D80000 has bits 17+19 = 0 → should not match Gayle filter.
        assert_eq!(g.read(0xD8_0000), 0);
        // $D90000 has bit 19 set but not bit 17 → should not match.
        assert_eq!(g.read(0xD9_0000), 0);
    }
}
