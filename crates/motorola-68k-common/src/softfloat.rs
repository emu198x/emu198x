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

/// IEEE rounding mode, matching SoftFloat's `float_rounding_mode` enum
/// (and the 68881/2 FPCR MODE field, bits 5-4, exactly):
/// `0` = round to nearest even, `1` = toward zero, `2` = toward −∞,
/// `3` = toward +∞.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum RoundingMode {
    /// Round to nearest, ties to even (FPCR `00`).
    NearestEven,
    /// Round toward zero / truncate (FPCR `01`).
    Zero,
    /// Round toward −∞ (FPCR `10`).
    Down,
    /// Round toward +∞ (FPCR `11`).
    Up,
}

impl RoundingMode {
    /// Decode the 2-bit FPCR rounding-mode field (bits 5-4).
    #[must_use]
    pub const fn from_fpcr_bits(bits: u8) -> Self {
        match bits & 0x3 {
            0 => Self::NearestEven,
            1 => Self::Zero,
            2 => Self::Down,
            _ => Self::Up,
        }
    }
}

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

// --- NaN handling (softfloat-specialize) ---

/// `floatx80_is_signaling_nan`: max exponent, non-zero fraction with the
/// MSB of the fraction (the QNaN bit) clear.
#[must_use]
fn is_signaling_nan(a: FpReg) -> bool {
    let a_low = a.low & !0x4000_0000_0000_0000;
    (a.high & 0x7FFF) == 0x7FFF && a_low.wrapping_shl(1) != 0 && a.low == a_low
}

/// `propagateFloatx80NaN`: given two values, one of which is a NaN, return
/// the NaN result the 68881/2 would. The `float_raise(invalid)` on a
/// signaling input is a side effect only (does not change the value) and is
/// deferred — see the FPSR TODO on [`round_and_pack_floatx80`].
#[must_use]
fn propagate_floatx80_nan(mut a: FpReg, mut b: FpReg) -> FpReg {
    let a_is_nan = a.is_nan();
    let a_is_signaling = is_signaling_nan(a);
    let b_is_nan = b.is_nan();
    // b_is_signaling only feeds the deferred float_raise — not computed.
    a.low |= 0xC000_0000_0000_0000;
    b.low |= 0xC000_0000_0000_0000;
    // TODO(FPSR): float_raise(invalid) if a_is_signaling | b_is_signaling.
    if a_is_nan {
        if a_is_signaling && b_is_nan { b } else { a }
    } else {
        b
    }
}

// --- Rounding core (softfloat.c) ---

/// `sub128`: subtract the 128-bit value (`b0`,`b1`) from (`a0`,`a1`) modulo
/// 2^128 (borrow out is lost).
#[must_use]
fn sub128(a0: u64, a1: u64, b0: u64, b1: u64) -> (u64, u64) {
    let z1 = a1.wrapping_sub(b1);
    let z0 = a0.wrapping_sub(b0).wrapping_sub(u64::from(a1 < b1));
    (z0, z1)
}

/// `roundAndPackFloatx80`: round the abstract value (`z_sign`, `z_exp`,
/// significand `z_sig0:z_sig1`) to `rounding_precision` bits (32, 64, or 80)
/// under `rounding_mode`, and pack it. The input significand must be
/// normalized or smaller.
///
/// This is a value-only port: the `float_exception_flags` /
/// `float_raise` side effects (inexact, overflow, underflow) are deferred —
/// they do not change the returned value. TODO(FPSR): fold them into the
/// FPSR EXC byte once the FPU exposes it.
#[must_use]
pub fn round_and_pack_floatx80(
    rounding_precision: i32,
    rounding_mode: RoundingMode,
    z_sign: bool,
    mut z_exp: i32,
    mut z_sig0: u64,
    z_sig1: u64,
) -> FpReg {
    let round_nearest_even = rounding_mode == RoundingMode::NearestEven;

    // Common overflow tail. `round_mask` carries the precision (32/64) max
    // finite significand; the precision-80 path jumps in with mask 0.
    let overflow = |round_mask: u64| -> FpReg {
        // TODO(FPSR): float_raise(overflow | inexact).
        if rounding_mode == RoundingMode::Zero
            || (z_sign && rounding_mode == RoundingMode::Up)
            || (!z_sign && rounding_mode == RoundingMode::Down)
        {
            return pack(z_sign, 0x7FFE, !round_mask);
        }
        pack(z_sign, 0x7FFF, 0x8000_0000_0000_0000)
    };

    if rounding_precision == 64 || rounding_precision == 32 {
        let (mut round_increment, mut round_mask) = if rounding_precision == 64 {
            (0x0000_0000_0000_0400_u64, 0x0000_0000_0000_07FF_u64)
        } else {
            (0x0000_0080_0000_0000_u64, 0x0000_00FF_FFFF_FFFF_u64)
        };
        z_sig0 |= u64::from(z_sig1 != 0);
        if !round_nearest_even {
            if rounding_mode == RoundingMode::Zero {
                round_increment = 0;
            } else {
                round_increment = round_mask;
                if z_sign {
                    if rounding_mode == RoundingMode::Up {
                        round_increment = 0;
                    }
                } else if rounding_mode == RoundingMode::Down {
                    round_increment = 0;
                }
            }
        }
        let mut round_bits = z_sig0 & round_mask;
        if (z_exp - 1) as u32 >= 0x7FFD {
            if 0x7FFE < z_exp || (z_exp == 0x7FFE && z_sig0.wrapping_add(round_increment) < z_sig0)
            {
                return overflow(round_mask);
            }
            if z_exp <= 0 {
                z_sig0 = shift64_right_jamming(z_sig0, 1 - z_exp);
                z_exp = 0;
                round_bits = z_sig0 & round_mask;
                // TODO(FPSR): underflow/inexact raises here.
                z_sig0 = z_sig0.wrapping_add(round_increment);
                if (z_sig0 as i64) < 0 {
                    z_exp = 1;
                }
                round_increment = round_mask + 1;
                if round_nearest_even && round_bits << 1 == round_increment {
                    round_mask |= round_increment;
                }
                z_sig0 &= !round_mask;
                return pack(z_sign, z_exp, z_sig0);
            }
        }
        // TODO(FPSR): inexact raise if round_bits.
        z_sig0 = z_sig0.wrapping_add(round_increment);
        if z_sig0 < round_increment {
            z_exp += 1;
            z_sig0 = 0x8000_0000_0000_0000;
        }
        round_increment = round_mask + 1;
        if round_nearest_even && round_bits << 1 == round_increment {
            round_mask |= round_increment;
        }
        z_sig0 &= !round_mask;
        if z_sig0 == 0 {
            z_exp = 0;
        }
        return pack(z_sign, z_exp, z_sig0);
    }

    // precision80
    let mut increment = (z_sig1 as i64) < 0;
    if !round_nearest_even {
        if rounding_mode == RoundingMode::Zero {
            increment = false;
        } else if z_sign {
            increment = rounding_mode == RoundingMode::Down && z_sig1 != 0;
        } else {
            increment = rounding_mode == RoundingMode::Up && z_sig1 != 0;
        }
    }
    if (z_exp - 1) as u32 >= 0x7FFD {
        if 0x7FFE < z_exp || (z_exp == 0x7FFE && z_sig0 == 0xFFFF_FFFF_FFFF_FFFF && increment) {
            return overflow(0);
        }
        if z_exp <= 0 {
            let (s0, s1) = shift64_extra_right_jamming(z_sig0, z_sig1, 1 - z_exp);
            z_sig0 = s0;
            let z_sig1 = s1;
            z_exp = 0;
            // TODO(FPSR): underflow/inexact raises here.
            if round_nearest_even {
                increment = (z_sig1 as i64) < 0;
            } else if z_sign {
                increment = rounding_mode == RoundingMode::Down && z_sig1 != 0;
            } else {
                increment = rounding_mode == RoundingMode::Up && z_sig1 != 0;
            }
            if increment {
                z_sig0 = z_sig0.wrapping_add(1);
                if z_sig1.wrapping_shl(1) == 0 && round_nearest_even {
                    z_sig0 &= !1;
                }
                if (z_sig0 as i64) < 0 {
                    z_exp = 1;
                }
            }
            return pack(z_sign, z_exp, z_sig0);
        }
    }
    // TODO(FPSR): inexact raise if z_sig1.
    if increment {
        z_sig0 = z_sig0.wrapping_add(1);
        if z_sig0 == 0 {
            z_exp += 1;
            z_sig0 = 0x8000_0000_0000_0000;
        } else if z_sig1.wrapping_shl(1) == 0 && round_nearest_even {
            z_sig0 &= !1;
        }
    } else if z_sig0 == 0 {
        z_exp = 0;
    }
    pack(z_sign, z_exp, z_sig0)
}

/// `normalizeRoundAndPackFloatx80`: like [`round_and_pack_floatx80`] but the
/// input significand need not be normalized.
#[must_use]
fn normalize_round_and_pack_floatx80(
    rounding_precision: i32,
    rounding_mode: RoundingMode,
    z_sign: bool,
    mut z_exp: i32,
    mut z_sig0: u64,
    mut z_sig1: u64,
) -> FpReg {
    if z_sig0 == 0 {
        z_sig0 = z_sig1;
        z_sig1 = 0;
        z_exp -= 64;
    }
    let shift_count = z_sig0.leading_zeros() as i32;
    let (z_sig0, z_sig1) = short_shift128_left(z_sig0, z_sig1, shift_count);
    z_exp -= shift_count;
    round_and_pack_floatx80(
        rounding_precision,
        rounding_mode,
        z_sign,
        z_exp,
        z_sig0,
        z_sig1,
    )
}

// --- Addition / subtraction (softfloat.c) ---

/// Shared `shiftRight1` + round-and-pack tail of `addFloatx80Sigs`.
#[must_use]
fn add_shift_right1_and_pack(
    precision: i32,
    mode: RoundingMode,
    z_sign: bool,
    z_exp: i32,
    z_sig0: u64,
    z_sig1: u64,
) -> FpReg {
    let (s0, s1) = shift64_extra_right_jamming(z_sig0, z_sig1, 1);
    round_and_pack_floatx80(
        precision,
        mode,
        z_sign,
        z_exp + 1,
        s0 | 0x8000_0000_0000_0000,
        s1,
    )
}

/// `addFloatx80Sigs`: add the absolute values of `a` and `b`, negating the
/// result if `z_sign` (ignored for a NaN result).
#[must_use]
fn add_floatx80_sigs(
    precision: i32,
    mode: RoundingMode,
    a: FpReg,
    b: FpReg,
    z_sign: bool,
) -> FpReg {
    let mut a_sig = frac(a);
    let a_exp = exp(a);
    let mut b_sig = frac(b);
    let b_exp = exp(b);
    let mut exp_diff = a_exp - b_exp;

    if 0 < exp_diff {
        if a_exp == 0x7FFF {
            if a_sig.wrapping_shl(1) != 0 {
                return propagate_floatx80_nan(a, b);
            }
            return a;
        }
        if b_exp == 0 {
            exp_diff -= 1;
        }
        let (bs, z_sig1) = shift64_extra_right_jamming(b_sig, 0, exp_diff);
        b_sig = bs;
        let z_exp = a_exp;
        let z_sig0 = a_sig.wrapping_add(b_sig);
        if (z_sig0 as i64) < 0 {
            return round_and_pack_floatx80(precision, mode, z_sign, z_exp, z_sig0, z_sig1);
        }
        return add_shift_right1_and_pack(precision, mode, z_sign, z_exp, z_sig0, z_sig1);
    }
    if exp_diff < 0 {
        if b_exp == 0x7FFF {
            if b_sig.wrapping_shl(1) != 0 {
                return propagate_floatx80_nan(a, b);
            }
            return pack(z_sign, 0x7FFF, 0x8000_0000_0000_0000);
        }
        if a_exp == 0 {
            exp_diff += 1;
        }
        let (as_, z_sig1) = shift64_extra_right_jamming(a_sig, 0, -exp_diff);
        a_sig = as_;
        let z_exp = b_exp;
        let z_sig0 = a_sig.wrapping_add(b_sig);
        if (z_sig0 as i64) < 0 {
            return round_and_pack_floatx80(precision, mode, z_sign, z_exp, z_sig0, z_sig1);
        }
        return add_shift_right1_and_pack(precision, mode, z_sign, z_exp, z_sig0, z_sig1);
    }
    // exp_diff == 0
    if a_exp == 0x7FFF {
        if (a_sig | b_sig).wrapping_shl(1) != 0 {
            return propagate_floatx80_nan(a, b);
        }
        return a;
    }
    let z_sig1 = 0;
    let z_sig0 = a_sig.wrapping_add(b_sig);
    if a_exp == 0 {
        let (e, s) = normalize_floatx80_subnormal(z_sig0);
        return round_and_pack_floatx80(precision, mode, z_sign, e, s, z_sig1);
    }
    add_shift_right1_and_pack(precision, mode, z_sign, a_exp, z_sig0, z_sig1)
}

/// `subFloatx80Sigs`: subtract the absolute values of `a` and `b`, negating
/// the result if `z_sign` (ignored for a NaN result).
#[must_use]
fn sub_floatx80_sigs(
    precision: i32,
    mode: RoundingMode,
    a: FpReg,
    b: FpReg,
    z_sign: bool,
) -> FpReg {
    let mut a_sig = frac(a);
    let mut a_exp = exp(a);
    let mut b_sig = frac(b);
    let mut b_exp = exp(b);
    let mut exp_diff = a_exp - b_exp;

    if 0 < exp_diff {
        // aExpBigger
        if a_exp == 0x7FFF {
            if a_sig.wrapping_shl(1) != 0 {
                return propagate_floatx80_nan(a, b);
            }
            return a;
        }
        if b_exp == 0 {
            exp_diff -= 1;
        }
        let (bs, z_sig1) = shift128_right_jamming(b_sig, 0, exp_diff);
        b_sig = bs;
        let (z_sig0, z_sig1) = sub128(a_sig, 0, b_sig, z_sig1);
        return normalize_round_and_pack_floatx80(precision, mode, z_sign, a_exp, z_sig0, z_sig1);
    }
    if exp_diff < 0 {
        // bExpBigger
        if b_exp == 0x7FFF {
            if b_sig.wrapping_shl(1) != 0 {
                return propagate_floatx80_nan(a, b);
            }
            return pack(!z_sign, 0x7FFF, 0x8000_0000_0000_0000);
        }
        if a_exp == 0 {
            exp_diff += 1;
        }
        let (as_, z_sig1) = shift128_right_jamming(a_sig, 0, -exp_diff);
        a_sig = as_;
        let (z_sig0, z_sig1) = sub128(b_sig, 0, a_sig, z_sig1);
        return normalize_round_and_pack_floatx80(precision, mode, !z_sign, b_exp, z_sig0, z_sig1);
    }
    // exp_diff == 0
    if a_exp == 0x7FFF {
        if (a_sig | b_sig).wrapping_shl(1) != 0 {
            return propagate_floatx80_nan(a, b);
        }
        // TODO(FPSR): float_raise(invalid) — inf − inf.
        return FpReg::new(0xFFFF, 0xFFFF_FFFF_FFFF_FFFF);
    }
    if a_exp == 0 {
        a_exp = 1;
        b_exp = 1;
    }
    let z_sig1 = 0;
    if b_sig < a_sig {
        // aBigger
        let (z_sig0, z_sig1) = sub128(a_sig, 0, b_sig, z_sig1);
        return normalize_round_and_pack_floatx80(precision, mode, z_sign, a_exp, z_sig0, z_sig1);
    }
    if a_sig < b_sig {
        // bBigger
        let (z_sig0, z_sig1) = sub128(b_sig, 0, a_sig, z_sig1);
        return normalize_round_and_pack_floatx80(precision, mode, !z_sign, b_exp, z_sig0, z_sig1);
    }
    pack(mode == RoundingMode::Down, 0, 0)
}

/// `floatx80_add`: add two extended-precision values.
#[must_use]
pub fn floatx80_add(precision: i32, mode: RoundingMode, a: FpReg, b: FpReg) -> FpReg {
    let a_sign = sign(a);
    let b_sign = sign(b);
    if a_sign == b_sign {
        add_floatx80_sigs(precision, mode, a, b, a_sign)
    } else {
        sub_floatx80_sigs(precision, mode, a, b, a_sign)
    }
}

/// `floatx80_sub`: subtract `b` from `a` at extended precision.
#[must_use]
pub fn floatx80_sub(precision: i32, mode: RoundingMode, a: FpReg, b: FpReg) -> FpReg {
    let a_sign = sign(a);
    let b_sign = sign(b);
    if a_sign == b_sign {
        sub_floatx80_sigs(precision, mode, a, b, a_sign)
    } else {
        add_floatx80_sigs(precision, mode, a, b, a_sign)
    }
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

    // --- Exact-value add/sub (no rounding occurs, so FPSR side effects are
    // irrelevant; results are bit-exact against Musashi by construction). ---

    const RN: RoundingMode = RoundingMode::NearestEven;
    const P80: i32 = 80;

    /// `n.0` for a small positive integer, via the exact int→fx80 path.
    fn fx(n: i32) -> FpReg {
        int32_to_floatx80(n)
    }

    #[test]
    fn add_one_plus_one_is_two() {
        assert_eq!(floatx80_add(P80, RN, fx(1), fx(1)), fx(2));
    }

    #[test]
    fn add_two_plus_one_is_three() {
        assert_eq!(floatx80_add(P80, RN, fx(2), fx(1)), fx(3));
        // 3.0 = 1.1b × 2^1 → exp 0x4000, sig 0xC000…
        assert_eq!(fx(3), FpReg::new(0x4000, 0xC000_0000_0000_0000));
    }

    #[test]
    fn add_is_commutative_on_exact_values() {
        assert_eq!(
            floatx80_add(P80, RN, fx(1), fx(2)),
            floatx80_add(P80, RN, fx(2), fx(1))
        );
    }

    #[test]
    fn sub_three_minus_one_is_two() {
        assert_eq!(floatx80_sub(P80, RN, fx(3), fx(1)), fx(2));
    }

    #[test]
    fn sub_equal_magnitude_is_positive_zero() {
        // 1.0 − 1.0 = +0.0 under round-to-nearest.
        assert_eq!(floatx80_sub(P80, RN, fx(1), fx(1)), FpReg::new(0, 0));
    }

    #[test]
    fn sub_equal_magnitude_is_negative_zero_rounding_down() {
        // The sign of an exact zero difference is − only when rounding down.
        assert_eq!(
            floatx80_sub(P80, RoundingMode::Down, fx(1), fx(1)),
            FpReg::new(0x8000, 0)
        );
    }

    #[test]
    fn add_opposite_signs_subtracts() {
        // 5 + (−2) = 3 — opposite signs route add → subFloatx80Sigs.
        assert_eq!(floatx80_add(P80, RN, fx(5), fx(-2)), fx(3));
    }

    #[test]
    fn sub_opposite_signs_adds() {
        // 2 − (−1) = 3 — opposite signs route sub → addFloatx80Sigs.
        assert_eq!(floatx80_sub(P80, RN, fx(2), fx(-1)), fx(3));
    }

    #[test]
    fn add_negatives() {
        // −2 + −3 = −5.
        assert_eq!(floatx80_add(P80, RN, fx(-2), fx(-3)), fx(-5));
    }

    #[test]
    fn add_zero_is_identity() {
        assert_eq!(floatx80_add(P80, RN, fx(7), FpReg::new(0, 0)), fx(7));
        assert_eq!(floatx80_add(P80, RN, FpReg::new(0, 0), fx(7)), fx(7));
    }

    #[test]
    fn add_large_exact_powers() {
        // 2^32 + 2^32 = 2^33. 2^32 = 1.0 × 2^32 → exp 0x3FFF + 32 = 0x401F.
        let two_p32 = FpReg::new(0x401F, 0x8000_0000_0000_0000);
        // 2^33 → exp 0x4020.
        assert_eq!(
            floatx80_add(P80, RN, two_p32, two_p32),
            FpReg::new(0x4020, 0x8000_0000_0000_0000)
        );
    }

    #[test]
    fn add_infinity_plus_finite_is_infinity() {
        let inf = FpReg::new(0x7FFF, 0x8000_0000_0000_0000);
        assert_eq!(floatx80_add(P80, RN, inf, fx(1)), inf);
        assert_eq!(floatx80_add(P80, RN, fx(1), inf), inf);
    }

    #[test]
    fn sub_infinities_is_default_nan() {
        let inf = FpReg::new(0x7FFF, 0x8000_0000_0000_0000);
        // +inf − +inf = default NaN.
        assert_eq!(
            floatx80_sub(P80, RN, inf, inf),
            FpReg::new(0xFFFF, 0xFFFF_FFFF_FFFF_FFFF)
        );
    }

    #[test]
    fn rounding_mode_decodes_fpcr_bits() {
        assert_eq!(RoundingMode::from_fpcr_bits(0), RoundingMode::NearestEven);
        assert_eq!(RoundingMode::from_fpcr_bits(1), RoundingMode::Zero);
        assert_eq!(RoundingMode::from_fpcr_bits(2), RoundingMode::Down);
        assert_eq!(RoundingMode::from_fpcr_bits(3), RoundingMode::Up);
    }
}
