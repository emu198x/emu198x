//! Custom-chipset register storage — Denise's slot.
//!
//! As of task #148 (the Agnus bitplane-DMA port), the chipset module
//! holds only the registers Denise owns: BPLCON1/2 (scroll, dual
//! playfield, priority) and the colour palette. Agnus owns DMACON,
//! BPLCON0, the bitplane and disk pointers, the DDF/DIW windows, and
//! the bitplane modulos. Paula owns INTENA / INTREQ / ADKCON and the
//! disk + audio + serial + POT registers.
//!
//! The Denise port (#150–#165) will move what's left out of here.

#[derive(Default)]
pub struct Chipset {
    /// `$DFF102` — bitplane control 1 (scroll, dual playfield).
    pub bplcon1: u16,
    /// `$DFF104` — bitplane control 2 (priority).
    pub bplcon2: u16,
    /// `$DFF180-$DFF1BE` — colour table (32 entries × 12-bit RGB,
    /// stored in u16 with high 4 bits unused).
    pub color: [u16; 32],
}

impl Chipset {
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Write one word to a custom register at the given offset.
    /// Offsets outside Denise's remaining range silently drop —
    /// machine dispatch routes Agnus and Paula registers before
    /// reaching us.
    pub fn write_word(&mut self, offset: u16, val: u16) {
        match offset {
            0x102 => self.bplcon1 = val,
            0x104 => self.bplcon2 = val,
            0x180..=0x1BE => {
                let idx = ((offset - 0x180) / 2) as usize;
                if idx < self.color.len() {
                    // OCS Denise: 12-bit colour; high nibble ignored.
                    self.color[idx] = val & 0x0FFF;
                }
            }
            _ => {}
        }
    }

    /// Read one word. All read-side mirrors (DMACONR, INTENAR,
    /// INTREQR, ADKCONR, DSKBYTR, …) live elsewhere now; this only
    /// returns floating-bus.
    #[must_use]
    pub fn read_word(&self, _offset: u16) -> u16 {
        0xFFFF
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn bplcon1_direct_write() {
        let mut c = Chipset::new();
        c.write_word(0x102, 0x0F0F);
        assert_eq!(c.bplcon1, 0x0F0F);
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
    fn unmapped_offsets_drop_silently() {
        let mut c = Chipset::new();
        c.write_word(0x000, 0xDEAD);
        c.write_word(0x1FE, 0xBEEF);
        assert_eq!(c.read_word(0x000), 0xFFFF);
    }
}
