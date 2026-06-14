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

// --- Bit-manipulation helpers (softfloat-macros) ---
//
// These mirror SoftFloat's shift routines exactly. The C relies on the
// host shift instruction masking the count (x86 SHL/SHR mask to the
// register width), so `<<`/`>>` by the register width is a no-op there;
// the ports use `wrapping_shl`/`wrapping_shr` to reproduce that on every
// platform (Rust's plain `<<`/`>>` would panic). All other shifts are
// already guarded to `< 64` by the surrounding branch, matching the C.

/// `shift64RightJamming`: shift `a` right by `count`, OR-ing any bits
/// shifted off into the LSB (sticky bit).
#[must_use]
pub fn shift64_right_jamming(a: u64, count: i32) -> u64 {
    if count == 0 {
        a
    } else if count < 64 {
        let c = count as u32;
        (a >> c) | u64::from(a.wrapping_shl(c.wrapping_neg() & 63) != 0)
    } else {
        u64::from(a != 0)
    }
}

/// `shift64ExtraRightJamming`: shift the 128-bit value (`a0`,`a1`) right
/// by `count`, returning the top 64 bits and a jammed extra 64 bits.
#[must_use]
pub fn shift64_extra_right_jamming(a0: u64, a1: u64, count: i32) -> (u64, u64) {
    if count == 0 {
        (a0, a1)
    } else if count < 64 {
        let c = count as u32;
        let neg = c.wrapping_neg() & 63;
        let z1 = a0.wrapping_shl(neg) | u64::from(a1 != 0);
        (a0 >> c, z1)
    } else {
        let z1 = if count == 64 {
            a0 | u64::from(a1 != 0)
        } else {
            u64::from((a0 | a1) != 0)
        };
        (0, z1)
    }
}

/// `shift128RightJamming`: shift the 128-bit value (`a0`,`a1`) right by
/// `count`, jamming bits shifted off into the LSB.
#[must_use]
pub fn shift128_right_jamming(a0: u64, a1: u64, count: i32) -> (u64, u64) {
    let neg = (count as u32).wrapping_neg() & 63;
    if count == 0 {
        (a0, a1)
    } else if count < 64 {
        let c = count as u32;
        let z1 = a0.wrapping_shl(neg) | (a1 >> c) | u64::from(a1.wrapping_shl(neg) != 0);
        (a0 >> c, z1)
    } else {
        let z1 = if count == 64 {
            a0 | u64::from(a1 != 0)
        } else if count < 128 {
            let c = (count & 63) as u32;
            (a0 >> c) | u64::from((a0.wrapping_shl(neg) | a1) != 0)
        } else {
            u64::from((a0 | a1) != 0)
        };
        (0, z1)
    }
}

/// `shortShift128Left`: shift the 128-bit value (`a0`,`a1`) left by
/// `count` (0..63).
#[must_use]
pub fn short_shift128_left(a0: u64, a1: u64, count: i32) -> (u64, u64) {
    let c = count as u32;
    let z1 = a1.wrapping_shl(c);
    let z0 = if count == 0 {
        a0
    } else {
        (a0 << c) | (a1 >> ((-count) as u32 & 63))
    };
    (z0, z1)
}

/// `normalizeFloatx80Subnormal`: normalize a subnormal significand,
/// returning `(exponent, significand)`. The caller passes a non-zero
/// `a_sig` in normal use; `wrapping_shl` keeps the zero case panic-free
/// and matches the host (count masked → no shift).
#[must_use]
pub fn normalize_floatx80_subnormal(a_sig: u64) -> (i32, u64) {
    let shift_count = a_sig.leading_zeros() as i32;
    (1 - shift_count, a_sig.wrapping_shl(shift_count as u32))
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

    #[test]
    fn shift64_right_jamming_basics() {
        assert_eq!(shift64_right_jamming(0xFF, 0), 0xFF);
        // 0x10 >> 1 = 0x8, nothing shifted off.
        assert_eq!(shift64_right_jamming(0x10, 1), 0x8);
        // 0x11 >> 1 = 0x8, low bit shifted off → jammed back to LSB.
        assert_eq!(shift64_right_jamming(0x11, 1), 0x9);
    }

    #[test]
    fn shift64_right_jamming_large_counts() {
        assert_eq!(shift64_right_jamming(1, 64), 1);
        assert_eq!(shift64_right_jamming(0, 64), 0);
        assert_eq!(shift64_right_jamming(0xDEAD, 100), 1);
        // The top bit shifted right by 63: result 1, nothing jammed.
        assert_eq!(shift64_right_jamming(0x8000_0000_0000_0000, 63), 1);
    }

    #[test]
    fn shift64_extra_right_jamming_by_one() {
        // The "shiftRight1" step in addFloatx80Sigs.
        let (z0, z1) = shift64_extra_right_jamming(0x8000_0000_0000_0000, 0, 1);
        assert_eq!(z0, 0x4000_0000_0000_0000);
        assert_eq!(z1, 0);
    }

    #[test]
    fn shift64_extra_right_jamming_count_64() {
        let (z0, z1) = shift64_extra_right_jamming(0xFF, 0x10, 64);
        assert_eq!(z0, 0);
        assert_eq!(z1, 0xFF | 1); // a0 | (a1 != 0)
    }

    #[test]
    fn normalize_subnormal_brings_msb_to_bit63() {
        let (exp, sig) = normalize_floatx80_subnormal(0x4000_0000_0000_0000);
        assert_eq!(exp, 0);
        assert_eq!(sig, 0x8000_0000_0000_0000);

        let (exp, sig) = normalize_floatx80_subnormal(1);
        assert_eq!(exp, 1 - 63);
        assert_eq!(sig, 0x8000_0000_0000_0000);
    }

    #[test]
    fn short_shift128_left_by_one() {
        let (z0, z1) = short_shift128_left(0x1, 0x8000_0000_0000_0000, 1);
        assert_eq!(z0, 3); // (1<<1) | (top bit of a1)
        assert_eq!(z1, 0);
    }

    #[test]
    fn shift128_right_jamming_count_64() {
        let (z0, z1) = shift128_right_jamming(0xABCD, 0x1, 64);
        assert_eq!(z0, 0);
        assert_eq!(z1, 0xABCD | 1);
    }
}
