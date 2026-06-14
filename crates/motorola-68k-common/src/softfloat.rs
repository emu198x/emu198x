//! Software 80-bit extended-precision floating point (`floatx80`).
//!
//! A faithful Rust port of the `floatx80` routines from Berkeley
//! SoftFloat (release 2b, John R. Hauser), as vendored by Musashi. The
//! port is deliberately a transliteration of the original algorithm so
//! that results are **bit-identical** to Musashi's FPU — which is our
//! validation oracle for the 68881/68882 (#112). Operations work
//! directly on [`FpReg`], whose `{high, low}` layout matches SoftFloat's
//! `floatx80` (`high` = sign + 15-bit exponent, `low` = 64-bit
//! significand with explicit integer bit).
//!
//! The arithmetic and the rounding core (`roundAndPackFloatx80`) are
//! ported incrementally; this module currently holds the foundation and
//! the exact integer→extended conversion. Transcendentals are not part
//! of SoftFloat and are handled separately.
//!
//! ## Original licence (Berkeley SoftFloat 2b)
//!
//! This Rust code is derived from Berkeley SoftFloat, which carries the
//! following notice:
//!
//! > This C source fragment is part of the SoftFloat IEC/IEEE Floating-
//! > point Arithmetic Package, Release 2b. Written by John R. Hauser.
//! >
//! > THIS SOFTWARE IS DISTRIBUTED AS IS, FOR FREE. Although reasonable
//! > effort has been made to avoid it, THIS SOFTWARE MAY CONTAIN FAULTS
//! > THAT WILL AT TIMES RESULT IN INCORRECT BEHAVIOR. … (BSD-style, see
//! > the upstream COPYING.txt). Redistributions must retain this notice.
//!
//! SoftFloat's BSD-style terms are compatible with this project's
//! GPL-2.0-or-later licence.

use crate::registers::FpReg;

/// Pack a sign, 15-bit exponent, and 64-bit significand into a
/// `floatx80` (`packFloatx80`).
#[must_use]
pub const fn pack(sign: bool, exp: i32, sig: u64) -> FpReg {
    // high = ((bits16)zSign << 15) + zExp  (wrapping, per the C cast)
    let high = ((sign as u16) << 15).wrapping_add(exp as u16);
    FpReg::new(high, sig)
}

/// Sign bit (`extractFloatx80Sign`).
#[must_use]
pub const fn sign(a: FpReg) -> bool {
    a.high & 0x8000 != 0
}

/// 15-bit biased exponent (`extractFloatx80Exp`).
#[must_use]
pub const fn exp(a: FpReg) -> i32 {
    (a.high & 0x7FFF) as i32
}

/// 64-bit significand including the explicit integer bit
/// (`extractFloatx80Frac`).
#[must_use]
pub const fn frac(a: FpReg) -> u64 {
    a.low
}

/// Convert a 32-bit signed integer to extended precision
/// (`int32_to_floatx80`). Exact for all `i32` — the value fits in the
/// 64-bit significand, so no rounding occurs.
#[must_use]
pub fn int32_to_floatx80(a: i32) -> FpReg {
    if a == 0 {
        return pack(false, 0, 0);
    }
    let z_sign = a < 0;
    let abs_a: u32 = a.unsigned_abs();
    // countLeadingZeros32(absA) + 32 — `absA` is non-zero here.
    let shift_count = abs_a.leading_zeros() + 32;
    let z_sig = u64::from(abs_a) << shift_count;
    pack(z_sign, 0x403E - shift_count as i32, z_sig)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn int32_zero_is_positive_zero() {
        assert_eq!(int32_to_floatx80(0), FpReg::new(0, 0));
    }

    #[test]
    fn int32_one() {
        // 1.0 = exponent bias (0x3FFF), significand = 1.000… (integer bit).
        assert_eq!(
            int32_to_floatx80(1),
            FpReg::new(0x3FFF, 0x8000_0000_0000_0000)
        );
    }

    #[test]
    fn int32_minus_one() {
        assert_eq!(
            int32_to_floatx80(-1),
            FpReg::new(0xBFFF, 0x8000_0000_0000_0000)
        );
    }

    #[test]
    fn int32_two() {
        // 2.0 = 1.0 × 2^1 → exponent 0x4000.
        assert_eq!(
            int32_to_floatx80(2),
            FpReg::new(0x4000, 0x8000_0000_0000_0000)
        );
    }

    #[test]
    fn int32_ten() {
        // 10 = 1.010b × 2^3 → exponent 0x4002, significand 0xA000…
        assert_eq!(
            int32_to_floatx80(10),
            FpReg::new(0x4002, 0xA000_0000_0000_0000)
        );
    }

    #[test]
    fn int32_max() {
        // 0x7FFF_FFFF = 2^31 − 1. clz32 = 1, shiftCount = 33,
        // exp = 0x403E − 33 = 0x401D.
        assert_eq!(
            int32_to_floatx80(0x7FFF_FFFF),
            FpReg::new(0x401D, 0xFFFF_FFFE_0000_0000)
        );
    }

    #[test]
    fn int32_min() {
        // i32::MIN = −2^31. unsigned_abs = 0x8000_0000, clz32 = 0,
        // shiftCount = 32, exp = 0x403E − 32 = 0x401E, sign set.
        assert_eq!(
            int32_to_floatx80(i32::MIN),
            FpReg::new(0xC01E, 0x8000_0000_0000_0000)
        );
    }

    #[test]
    fn extract_helpers_round_trip() {
        let v = pack(true, 0x4002, 0xA000_0000_0000_0000);
        assert!(sign(v));
        assert_eq!(exp(v), 0x4002);
        assert_eq!(frac(v), 0xA000_0000_0000_0000);
    }
}
