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
    fn unmapped_register_offsets_drop_silently() {
        let mut c = Chipset::new();
        // No panic, no error — just dropped.
        c.write_word(0x000, 0xDEAD);
        c.write_word(0x1FE, 0xBEEF);
        assert_eq!(c.read_word(0x000), 0xFFFF);
    }
}
