//! Amiga blitter.
//!
//! Performs rectangular DMA operations on chip RAM. Triggered by writing
//! BLTSIZE. Currently executes instantly (no DMA timing) which is sufficient
//! for boot.
//!
//! The blitter has 4 channels:
//! - A, B, C: source channels (read from chip RAM or data registers)
//! - D: destination channel (write to chip RAM)
//!
//! BLTCON0 bits 7-0 select the logic function (minterm) applied to A, B, C.

#![allow(clippy::cast_possible_truncation, clippy::cast_sign_loss, clippy::cast_possible_wrap)]

use crate::memory::Memory;

/// Blitter state.
pub struct Blitter {
    pub bltcon0: u16,
    pub bltcon1: u16,
    pub bltafwm: u16,
    pub bltalwm: u16,
    pub bltcpt: u32,
    pub bltbpt: u32,
    pub bltapt: u32,
    pub bltdpt: u32,
    pub bltsize: u16,
    pub bltcmod: u16,
    pub bltbmod: u16,
    pub bltamod: u16,
    pub bltdmod: u16,
    pub bltcdat: u16,
    pub bltbdat: u16,
    pub bltadat: u16,
    /// True while a blit is in progress (always false for instant mode).
    busy: bool,
    /// Blitter zero flag (BLTDDAT was all zero).
    pub bzero: bool,
}

impl Blitter {
    #[must_use]
    pub fn new() -> Self {
        Self {
            bltcon0: 0,
            bltcon1: 0,
            bltafwm: 0xFFFF,
            bltalwm: 0xFFFF,
            bltcpt: 0,
            bltbpt: 0,
            bltapt: 0,
            bltdpt: 0,
            bltsize: 0,
            bltcmod: 0,
            bltbmod: 0,
            bltamod: 0,
            bltdmod: 0,
            bltcdat: 0,
            bltbdat: 0,
            bltadat: 0,
            busy: false,
            bzero: true,
        }
    }

    /// Is the blitter busy?
    #[must_use]
    pub const fn is_busy(&self) -> bool {
        self.busy
    }

    /// Execute a blit instantly. Called when BLTSIZE is written.
    pub fn do_blit(&mut self, memory: &mut Memory) {
        let height = (self.bltsize >> 6) & 0x03FF;
        let width = self.bltsize & 0x003F;

        // Height/width of 0 means 1024/64 respectively.
        let h = if height == 0 { 1024u32 } else { u32::from(height) };
        let w = if width == 0 { 64u32 } else { u32::from(width) };

        #[cfg(debug_assertions)]
        {
            let use_d = self.bltcon0 & 0x0100 != 0;
            if use_d {
                let d_start = self.bltdpt & 0x7FFFE;
                let d_mod = self.bltdmod as i16 as i32;
                let d_end = d_start as i32 + (h as i32 * (w as i32 * 2 + d_mod));
                if d_start <= 0x0500 && d_end as u32 >= 0x0420 {
                    eprintln!("  BLIT covers copinit region! D=${:06X} size={}x{} mod={} end~${:06X} con0=${:04X}",
                        d_start, w, h, d_mod, d_end, self.bltcon0);
                }
            }
        }

        let use_a = self.bltcon0 & 0x0800 != 0;
        let use_b = self.bltcon0 & 0x0400 != 0;
        let use_c = self.bltcon0 & 0x0200 != 0;
        let use_d = self.bltcon0 & 0x0100 != 0;
        let minterm = (self.bltcon0 & 0xFF) as u8;
        let a_shift = (self.bltcon0 >> 12) & 0xF;
        let b_shift = (self.bltcon1 >> 12) & 0xF;

        let desc = self.bltcon1 & 0x0002 != 0; // Descending mode

        let a_mod = self.bltamod as i16 as i32;
        let b_mod = self.bltbmod as i16 as i32;
        let c_mod = self.bltcmod as i16 as i32;
        let d_mod = self.bltdmod as i16 as i32;

        let mut apt = self.bltapt;
        let mut bpt = self.bltbpt;
        let mut cpt = self.bltcpt;
        let mut dpt = self.bltdpt;

        let mut a_prev = 0u16;
        let mut b_prev = 0u16;
        let mut all_zero = true;

        for _row in 0..h {
            for col in 0..w {
                // Read source channels
                let a_raw = if use_a {
                    let v = memory.read_chip_word(apt);
                    if desc { apt = apt.wrapping_sub(2); } else { apt = apt.wrapping_add(2); }
                    v
                } else {
                    self.bltadat
                };

                let b_raw = if use_b {
                    let v = memory.read_chip_word(bpt);
                    if desc { bpt = bpt.wrapping_sub(2); } else { bpt = bpt.wrapping_add(2); }
                    v
                } else {
                    self.bltbdat
                };

                let c_val = if use_c {
                    let v = memory.read_chip_word(cpt);
                    if desc { cpt = cpt.wrapping_sub(2); } else { cpt = cpt.wrapping_add(2); }
                    v
                } else {
                    self.bltcdat
                };

                // Apply first/last word masks to A
                let mut a_masked = a_raw;
                if col == 0 {
                    a_masked &= self.bltafwm;
                }
                if col == w - 1 {
                    a_masked &= self.bltalwm;
                }

                // Apply barrel shift to A
                let a_val = if a_shift == 0 {
                    a_prev = a_masked;
                    a_masked
                } else {
                    let combined = (u32::from(a_prev) << 16) | u32::from(a_masked);
                    a_prev = a_masked;
                    if desc {
                        (combined << a_shift >> 16) as u16
                    } else {
                        (combined >> a_shift) as u16
                    }
                };

                // Apply barrel shift to B
                let b_val = if b_shift == 0 {
                    b_prev = b_raw;
                    b_raw
                } else {
                    let combined = (u32::from(b_prev) << 16) | u32::from(b_raw);
                    b_prev = b_raw;
                    if desc {
                        (combined << b_shift >> 16) as u16
                    } else {
                        (combined >> b_shift) as u16
                    }
                };

                // Apply minterm logic function
                let d_val = apply_minterm(a_val, b_val, c_val, minterm);

                if d_val != 0 {
                    all_zero = false;
                }

                // Write to destination
                if use_d {
                    memory.write_chip_word(dpt, d_val);
                    if desc { dpt = dpt.wrapping_sub(2); } else { dpt = dpt.wrapping_add(2); }
                }
            }

            // Apply modulos at end of each row
            if use_a { apt = (apt as i32).wrapping_add(a_mod) as u32; }
            if use_b { bpt = (bpt as i32).wrapping_add(b_mod) as u32; }
            if use_c { cpt = (cpt as i32).wrapping_add(c_mod) as u32; }
            if use_d { dpt = (dpt as i32).wrapping_add(d_mod) as u32; }

            // Reset shift pipeline at start of each row
            a_prev = 0;
            b_prev = 0;
        }

        // Update pointer registers
        self.bltapt = apt;
        self.bltbpt = bpt;
        self.bltcpt = cpt;
        self.bltdpt = dpt;
        self.bzero = all_zero;
    }
}

/// Apply the 8-bit minterm logic function to three source words.
///
/// Each bit of the minterm selects a combination of A, B, C:
/// - Bit 7: A & B & C
/// - Bit 6: A & B & ~C
/// - Bit 5: A & ~B & C
/// - Bit 4: A & ~B & ~C
/// - Bit 3: ~A & B & C
/// - Bit 2: ~A & B & ~C
/// - Bit 1: ~A & ~B & C
/// - Bit 0: ~A & ~B & ~C
fn apply_minterm(a: u16, b: u16, c: u16, minterm: u8) -> u16 {
    let mut result = 0u16;
    if minterm & 0x80 != 0 { result |= a & b & c; }
    if minterm & 0x40 != 0 { result |= a & b & !c; }
    if minterm & 0x20 != 0 { result |= a & !b & c; }
    if minterm & 0x10 != 0 { result |= a & !b & !c; }
    if minterm & 0x08 != 0 { result |= !a & b & c; }
    if minterm & 0x04 != 0 { result |= !a & b & !c; }
    if minterm & 0x02 != 0 { result |= !a & !b & c; }
    if minterm & 0x01 != 0 { result |= !a & !b & !c; }
    result
}

impl Default for Blitter {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn minterm_zero_clears() {
        assert_eq!(apply_minterm(0xFFFF, 0xFFFF, 0xFFFF, 0x00), 0x0000);
    }

    #[test]
    fn minterm_f0_copies_a() {
        assert_eq!(apply_minterm(0xABCD, 0x1234, 0x5678, 0xF0), 0xABCD);
    }

    #[test]
    fn minterm_cc_copies_b() {
        assert_eq!(apply_minterm(0xABCD, 0x1234, 0x5678, 0xCC), 0x1234);
    }

    #[test]
    fn minterm_aa_copies_c() {
        assert_eq!(apply_minterm(0xABCD, 0x1234, 0x5678, 0xAA), 0x5678);
    }

    #[test]
    fn minterm_ca_cookie_cut() {
        // D = (A & B) | (~A & C)
        let a = 0xFF00u16;
        let b = 0x1234u16;
        let c = 0x5678u16;
        let expected = (a & b) | (!a & c);
        assert_eq!(apply_minterm(a, b, c, 0xCA), expected);
    }
}
