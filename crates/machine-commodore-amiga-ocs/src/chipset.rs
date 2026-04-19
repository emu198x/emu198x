//! Custom-chipset registers — minimal storage layer (M2).
//!
//! At M2 the chipset is just a bag of register variables, with the
//! correct write semantics (set/clear for INTENA/INTREQ/DMACON/ADKCON;
//! direct word for everything else). No DMA, no copper, no display
//! pipeline yet. Behaviour lands in later milestones (M6+).
//!
//! Custom-register address space is `$DF_F000-$DF_F1FE`. The low 9
//! bits of the address (after subtracting the base) select the
//! register; we discard the high address bits.

#[derive(Default)]
pub struct Chipset {
    /// `$DFF096` — DMA control.
    pub dmacon: u16,
    /// `$DFF09A` — interrupt enable mask.
    pub intena: u16,
    /// `$DFF09C` — interrupt request lines.
    pub intreq: u16,
    /// `$DFF09E` — audio + disk control (peripheral DMA flags).
    pub adkcon: u16,
    /// `$DFF100` — bitplane control 0.
    pub bplcon0: u16,
    /// `$DFF180-$DFF1BE` — colour table (32 entries × 12-bit RGB,
    /// stored in u16 with high 4 bits unused).
    pub color: [u16; 32],
}

impl Chipset {
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Write one word to a custom register at the given offset
    /// (relative to `$DFF000`). Offsets outside the documented range
    /// silently drop.
    pub fn write_word(&mut self, offset: u16, val: u16) {
        match offset {
            0x096 => write_set_clear(&mut self.dmacon, val),
            0x09A => write_set_clear(&mut self.intena, val),
            0x09C => write_set_clear(&mut self.intreq, val),
            0x09E => write_set_clear(&mut self.adkcon, val),
            0x100 => self.bplcon0 = val,
            0x180..=0x1BE => {
                let idx = ((offset - 0x180) / 2) as usize;
                if idx < self.color.len() {
                    // OCS Denise stores 12-bit colour; high nibble is
                    // ignored on read AND write per hardware ref.
                    self.color[idx] = val & 0x0FFF;
                }
            }
            _ => {}
        }
    }

    /// Read one word from a custom register. Read-only side and
    /// write-only side share offsets ($096 = DMACON write / DMACONR
    /// read at $002; etc) — return the stored bookkeeping value for
    /// the read-side offsets and floating bus elsewhere.
    #[must_use]
    pub fn read_word(&self, offset: u16) -> u16 {
        match offset {
            0x002 => self.dmacon, // DMACONR
            0x01C => self.intena, // INTENAR
            0x01E => self.intreq, // INTREQR
            0x010 => self.adkcon, // ADKCONR
            _ => 0xFFFF,
        }
    }
}

/// Set/clear write semantics for INTENA/INTREQ/DMACON/ADKCON.
/// Bit 15 = 1 → set the bits in val[14..0]; bit 15 = 0 → clear them.
fn write_set_clear(reg: &mut u16, val: u16) {
    if val & 0x8000 != 0 {
        *reg |= val & 0x7FFF;
    } else {
        *reg &= !(val & 0x7FFF);
    }
}

impl Chipset {
    /// Compute the 68000 IPL (interrupt priority level, 0-7) the
    /// chipset is requesting based on `INTREQ & INTENA`. Per Paula:
    ///
    ///   bit 0 TBE     → level 1
    ///   bit 1 DSKBLK  → level 1
    ///   bit 2 SOFT    → level 1
    ///   bit 3 PORTS   → level 2
    ///   bit 4 COPER   → level 3
    ///   bit 5 VERTB   → level 3
    ///   bit 6 BLIT    → level 3
    ///   bit 7-10 AUDx → level 4
    ///   bit 11 RBF    → level 5
    ///   bit 12 DSKSYN → level 5
    ///   bit 13 EXTER  → level 6
    ///
    /// Bit 14 (INTEN master enable) gates everything except level 7
    /// (NMI; not used by Amiga in the standard config).
    #[must_use]
    pub fn compute_ipl(&self) -> u8 {
        if self.intena & 0x4000 == 0 {
            return 0;
        }
        let active = self.intreq & self.intena & 0x3FFF;
        if active & 0x2000 != 0 { 6 }      // EXTER
        else if active & 0x1800 != 0 { 5 } // RBF, DSKSYN
        else if active & 0x0780 != 0 { 4 } // AUD0..3
        else if active & 0x0070 != 0 { 3 } // COPER, VERTB, BLIT
        else if active & 0x0008 != 0 { 2 } // PORTS
        else if active & 0x0007 != 0 { 1 } // TBE, DSKBLK, SOFT
        else { 0 }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn intena_set_clear() {
        let mut c = Chipset::new();
        c.write_word(0x09A, 0x8042); // set bit 6 + bit 1
        assert_eq!(c.intena, 0x0042);
        c.write_word(0x09A, 0x8004); // also set bit 2
        assert_eq!(c.intena, 0x0046);
        c.write_word(0x09A, 0x0040); // clear bit 6
        assert_eq!(c.intena, 0x0006);
    }

    #[test]
    fn dmacon_independent_from_intena() {
        let mut c = Chipset::new();
        c.write_word(0x096, 0x8200); // DMAEN
        c.write_word(0x09A, 0x8000); // INTENA master only
        assert_eq!(c.dmacon, 0x0200);
        assert_eq!(c.intena, 0x0000);
    }

    #[test]
    fn bplcon0_direct_write() {
        let mut c = Chipset::new();
        c.write_word(0x100, 0x1302);
        assert_eq!(c.bplcon0, 0x1302);
        c.write_word(0x100, 0x0000);
        assert_eq!(c.bplcon0, 0x0000);
    }

    #[test]
    fn color_table_masks_to_12_bits() {
        let mut c = Chipset::new();
        c.write_word(0x180, 0xFFFF);
        assert_eq!(c.color[0], 0x0FFF);
        c.write_word(0x182, 0x0444);
        assert_eq!(c.color[1], 0x0444);
    }

    #[test]
    fn read_side_offsets_return_bookkeeping_values() {
        let mut c = Chipset::new();
        c.write_word(0x09A, 0x8042);
        assert_eq!(c.read_word(0x01C), 0x0042); // INTENAR mirrors INTENA
        c.write_word(0x096, 0x8200);
        assert_eq!(c.read_word(0x002), 0x0200); // DMACONR mirrors DMACON
    }

    #[test]
    fn compute_ipl_respects_master_enable_and_priority() {
        let mut c = Chipset::new();
        c.intreq = 0x0020; // VERTB requested
        // No master enable — IPL should be 0.
        assert_eq!(c.compute_ipl(), 0);
        c.intena = 0x4020; // master + VERTB
        assert_eq!(c.compute_ipl(), 3);
        // Add EXTER (level 6) — should win over VERTB.
        c.intena = 0x6020;
        c.intreq = 0x2020;
        assert_eq!(c.compute_ipl(), 6);
        // Drop EXTER request.
        c.intreq = 0x0020;
        assert_eq!(c.compute_ipl(), 3);
    }

    #[test]
    fn unmapped_register_offsets_drop_silently() {
        let mut c = Chipset::new();
        // No panic, no error — just dropped.
        c.write_word(0x000, 0xDEAD);
        c.write_word(0x1FE, 0xBEEF);
        assert_eq!(c.read_word(0x000), 0xFFFF);
    }
}
