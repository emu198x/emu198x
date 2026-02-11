//! Timing helpers and BCD arithmetic for the 68000.
//!
//! Contains Jorge Cwik's restoring division cycle algorithm and
//! BCD add/subtract/negate operations used by ABCD/SBCD/NBCD.

use crate::cpu::Cpu68000;
use crate::flags::X;

impl Cpu68000 {
    /// Compute exact DIVU cycle timing based on Jorge Cwik's algorithm.
    /// Returns total clock cycles for the division computation.
    pub(crate) fn divu_cycles(dividend: u32, divisor: u16) -> u8 {
        // Overflow case
        if (dividend >> 16) >= u32::from(divisor) {
            return 10;
        }

        let mut mcycles: u32 = 38;
        let hdivisor = u32::from(divisor) << 16;
        let mut dvd = dividend;

        for _ in 0..15 {
            let temp = dvd;
            dvd <<= 1;

            if temp & 0x8000_0000 != 0 {
                // Carry from shift — subtract divisor (no extra cycles)
                dvd = dvd.wrapping_sub(hdivisor);
            } else {
                // No carry — 2 extra cycles for the comparison step
                mcycles += 2;
                if dvd >= hdivisor {
                    // Subtraction succeeds — save 1 cycle
                    dvd = dvd.wrapping_sub(hdivisor);
                    mcycles -= 1;
                }
            }
        }
        (mcycles * 2) as u8
    }

    /// Compute exact DIVS cycle timing based on Jorge Cwik's algorithm.
    /// Returns total clock cycles for the division computation.
    pub(crate) fn divs_cycles(dividend: i32, divisor: i16) -> u8 {
        let mut mcycles: u32 = 6;
        if dividend < 0 {
            mcycles += 1;
        }

        // Overflow check using absolute values
        let abs_dividend = (dividend as i64).unsigned_abs() as u32;
        let abs_divisor = (divisor as i32).unsigned_abs() as u16;

        if (abs_dividend >> 16) >= u32::from(abs_divisor) {
            return ((mcycles + 2) * 2) as u8;
        }

        // Compute absolute quotient for bit-counting
        let mut aquot = abs_dividend / u32::from(abs_divisor);

        mcycles += 55;

        if divisor >= 0 {
            if dividend >= 0 {
                mcycles -= 1;
            } else {
                mcycles += 1;
            }
        }

        // Count 15 MSBs of absolute quotient — each 0-bit adds 1 mcycle
        for _ in 0..15 {
            if (aquot as i16) >= 0 {
                mcycles += 1;
            }
            aquot <<= 1;
        }
        (mcycles * 2) as u8
    }

    /// Perform BCD addition: src + dst + extend.
    /// Returns (result, carry, overflow).
    pub(crate) fn bcd_add(&self, src: u8, dst: u8, extend: u8) -> (u8, bool, bool) {
        // Low nibble: binary add then correct
        let low_sum = (dst & 0x0F) + (src & 0x0F) + extend;
        let corf: u16 = if low_sum > 9 { 6 } else { 0 };

        // Full binary sum (before any correction)
        let uncorrected = u16::from(dst) + u16::from(src) + u16::from(extend);

        // Carry: compute from high digit sum including full carry from low
        let low_corrected = low_sum + if low_sum > 9 { 6 } else { 0 };
        let low_carry = low_corrected >> 4;
        let high_sum = (dst >> 4) + (src >> 4) + low_carry;
        let carry = high_sum > 9;

        // Result: apply low correction, then high correction if carry
        let result = if carry {
            uncorrected + corf + 0x60
        } else {
            uncorrected + corf
        };

        // V: set when uncorrected bit 7 was 0 but corrected bit 7 is 1
        let overflow = (!uncorrected & result & 0x80) != 0;

        (result as u8, carry, overflow)
    }

    /// Perform BCD subtraction: dst - src - extend.
    /// Returns (result, borrow, overflow).
    pub(crate) fn bcd_sub(&self, dst: u8, src: u8, extend: u8) -> (u8, bool, bool) {
        // Binary subtraction first
        let uncorrected = dst.wrapping_sub(src).wrapping_sub(extend);

        let mut result = uncorrected;

        // Low nibble correction: if low nibble would have underflowed
        let low_borrowed = (dst & 0x0F) < (src & 0x0F).saturating_add(extend);
        if low_borrowed {
            result = result.wrapping_sub(6);
        }

        // High nibble correction: only if the original high nibble underflowed
        let high_borrowed = (dst >> 4) < (src >> 4) + u8::from(low_borrowed);
        if high_borrowed {
            result = result.wrapping_sub(0x60);
        }

        // Borrow: set if either the original high nibble underflowed, OR
        // the low nibble correction (-6) caused the whole byte to wrap.
        let low_correction_wraps = low_borrowed && uncorrected < 6;
        let borrow = high_borrowed || low_correction_wraps;

        // V: set when BCD correction flips bit 7 from 1 to 0
        let overflow = (uncorrected & !result & 0x80) != 0;

        (result, borrow, overflow)
    }

    /// Perform NBCD: negate BCD (0 - src - X).
    /// Returns (result, borrow, overflow).
    pub(crate) fn nbcd(&self, src: u8, extend: u8) -> (u8, bool, bool) {
        self.bcd_sub(0, src, extend)
    }

    /// Get the current X flag as 0 or 1.
    #[must_use]
    pub(crate) fn x_flag(&self) -> u8 {
        u8::from(self.regs.sr & X != 0)
    }
}
