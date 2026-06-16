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

// --- Exception flags (softfloat `float_flag_*` / `float_exception_flags`) ---
//
// SoftFloat reports IEEE exceptions through a global `float_exception_flags`
// that `float_raise` ORs into; the value-returning routines are otherwise
// pure. This port mirrors that exactly with a thread-local accumulator —
// matching the C structure keeps the transliteration faithful, so the flags
// our routines raise are bit-identical to softfloat.c's (the validation
// oracle). The 68881/2 FPU layer reads them after each operation and folds
// them into the FPSR EXC/AEXC bytes. Each op is synchronous within a thread
// (clear → compute → take), so the thread-local is race-free.

/// SoftFloat exception flag bits (`float_flag_*` in softfloat.h).
pub mod flag {
    /// Invalid operation (`float_flag_invalid`).
    pub const INVALID: u8 = 0x01;
    /// Denormalised input (`float_flag_denormal`).
    pub const DENORMAL: u8 = 0x02;
    /// Divide by zero (`float_flag_divbyzero`).
    pub const DIVBYZERO: u8 = 0x04;
    /// Overflow (`float_flag_overflow`).
    pub const OVERFLOW: u8 = 0x08;
    /// Underflow (`float_flag_underflow`).
    pub const UNDERFLOW: u8 = 0x10;
    /// Inexact result (`float_flag_inexact`).
    pub const INEXACT: u8 = 0x20;
    /// Inexact decimal conversion (`float_flag_decimal`) — raised by the
    /// packed-decimal ↔ `floatx80` conversions when the decimal value cannot
    /// be represented exactly. Maps to the 68881/2 FPSR INEX1 bit.
    pub const DECIMAL: u8 = 0x80;
}

thread_local! {
    static EXCEPTION_FLAGS: core::cell::Cell<u8> = const { core::cell::Cell::new(0) };
    // True when a signalling NaN was an *input* to the operation. SoftFloat
    // collapses signalling-NaN and operational invalids into the one
    // `invalid` flag; the 68881/2 splits them into SNAN vs OPERR, so we track
    // the signalling-input cause separately. It is NOT part of the
    // softfloat-compatible flag byte, so the C-diff oracle is unaffected.
    static SIGNALING_INPUT: core::cell::Cell<bool> = const { core::cell::Cell::new(false) };
}

/// Clear the accumulated exception state. Call before an operation whose
/// flags you intend to read.
pub fn clear_exception_flags() {
    EXCEPTION_FLAGS.with(|f| f.set(0));
    SIGNALING_INPUT.with(|f| f.set(false));
}

/// Read the accumulated exception flags without clearing them
/// (`float_exception_flags` snapshot).
#[must_use]
pub fn exception_flags() -> u8 {
    EXCEPTION_FLAGS.with(core::cell::Cell::get)
}

/// Read and clear the accumulated exception flags in one step. (Does not
/// touch the signalling-input marker; the next `clear` resets it.)
#[must_use]
pub fn take_exception_flags() -> u8 {
    EXCEPTION_FLAGS.with(|f| f.replace(0))
}

/// Whether a signalling NaN was an input to the operation since the last
/// `clear_exception_flags`. Distinguishes the 68881/2 SNAN cause from OPERR.
#[must_use]
pub fn signaling_nan_input() -> bool {
    SIGNALING_INPUT.with(core::cell::Cell::get)
}

/// Accumulate exception flags (`float_raise`). Internal to the port; also
/// used by the FPSP transcendentals in [`crate::softfloat_fpsp`].
pub(crate) fn float_raise(flags: u8) {
    EXCEPTION_FLAGS.with(|f| f.set(f.get() | flags));
}

/// Raise `invalid` for a signalling-NaN *input*, also recording the
/// signalling-NaN cause so the FPU layer can set SNAN rather than OPERR.
fn raise_signaling_nan() {
    float_raise(flag::INVALID);
    SIGNALING_INPUT.with(|f| f.set(true));
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
    // 68k (SOFTFLOAT_68K): `-shiftCount`, not Berkeley's generic `1 -
    // shiftCount`. The extended-format denormal sits one binary exponent
    // lower on the 68881/2 than the generic IEEE single/double convention.
    (-shift_count, a_sig.wrapping_shl(shift_count as u32))
}

/// `floatx80_is_unnormal` (softfloat-specialize.h): a non-canonical extended
/// value with a normal-range exponent but a clear explicit integer bit. The
/// 68881/2 normalises these operands before use.
#[must_use]
pub fn floatx80_is_unnormal(a: FpReg) -> bool {
    let e = exp(a);
    e > 0 && e < 0x7FFF && frac(a) & 0x8000_0000_0000_0000 == 0
}

/// `floatx80_is_denormal` (softfloat-specialize.h): a true extended denormal —
/// zero exponent, clear integer bit, non-zero fraction.
#[must_use]
pub fn floatx80_is_denormal(a: FpReg) -> bool {
    exp(a) == 0 && frac(a) & 0x8000_0000_0000_0000 == 0 && frac(a).wrapping_shl(1) != 0
}

/// `floatx80_normalize` (softfloat.cpp): shift an unnormal's significand left
/// until the integer bit is set, decrementing the exponent. Infinities, NaNs,
/// and zero-exponent values are returned unchanged (the 68881/2 leaves
/// denormals as-is; only unnormals are renormalised). An unnormal-zero (normal
/// exponent, zero significand) collapses to a signed zero.
#[must_use]
pub fn floatx80_normalize(a: FpReg) -> FpReg {
    let a_sign = sign(a);
    let mut a_exp = exp(a);
    let a_sig = frac(a);
    if a_exp == 0x7FFF || a_exp == 0 {
        return a;
    }
    if a_sig == 0 {
        return pack(a_sign, 0, 0);
    }
    let mut shift_count = a_sig.leading_zeros() as i32;
    if shift_count > a_exp {
        shift_count = a_exp;
    }
    a_exp -= shift_count;
    pack(a_sign, a_exp, a_sig.wrapping_shl(shift_count as u32))
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

/// `float32_is_signaling_nan`: max exponent, non-zero fraction, quiet bit
/// (fraction MSB) clear.
#[must_use]
fn float32_is_signaling_nan(a: u32) -> bool {
    ((a >> 22) & 0x1FF) == 0x1FE && (a & 0x003F_FFFF) != 0
}

/// `float64_is_signaling_nan`: max exponent, non-zero fraction, quiet bit
/// clear.
#[must_use]
fn float64_is_signaling_nan(a: u64) -> bool {
    ((a >> 51) & 0xFFF) == 0xFFE && (a & 0x0007_FFFF_FFFF_FFFF) != 0
}

/// `propagateFloatx80NaN`: given two values, one of which is a NaN, return
/// the NaN result the 68881/2 would. The `float_raise(invalid)` on a
/// signaling input is a side effect only (does not change the value) and is
/// deferred — see the FPSR TODO on [`round_and_pack_floatx80`].
#[must_use]
fn propagate_floatx80_nan(mut a: FpReg, mut b: FpReg) -> FpReg {
    let a_is_nan = a.is_nan();
    let a_is_signaling = is_signaling_nan(a);
    let b_is_signaling = is_signaling_nan(b);
    // 68k (SOFTFLOAT_68K): quiet by setting the quiet bit (62) only, leaving
    // the explicit integer bit (63) untouched, and return operand A's NaN if
    // A is a NaN (else B's). Generic Berkeley quiets bits 63+62 and favours B.
    a.low |= 0x4000_0000_0000_0000;
    b.low |= 0x4000_0000_0000_0000;
    if a_is_signaling || b_is_signaling {
        raise_signaling_nan();
    }
    if a_is_nan { a } else { b }
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

    // Shared overflow tail (SOFTFLOAT_68K). `round_mask` carries the precision
    // (32/64) max-finite significand mask (0 for extended). Raises OVERFLOW
    // unconditionally and INEXACT only when significand bits are actually
    // discarded; produces the 68k infinity ($7FFF:0, integer bit clear) or the
    // correctly-signed largest finite under directed rounding.
    let overflow = |round_mask: u64, sig0: u64, sig1: u64| -> FpReg {
        float_raise(flag::OVERFLOW);
        if (sig0 & round_mask) != 0 || sig1 != 0 {
            float_raise(flag::INEXACT);
        }
        if rounding_mode == RoundingMode::Zero
            || (z_sign && rounding_mode == RoundingMode::Up)
            || (!z_sign && rounding_mode == RoundingMode::Down)
        {
            return pack(z_sign, 0x7FFE, !round_mask);
        }
        pack(z_sign, 0x7FFF, 0) // 68k infinity: integer bit clear
    };

    if rounding_precision == 64 || rounding_precision == 32 {
        // The single/double rounder clamps the *exponent* to the target range
        // via `exp_offset` (single max = 0x7FFE−0x3F80 = 0x407E, min normal =
        // 0x3F81), so a value beyond single/double range overflows to ∞ / the
        // format max or underflows to its denormal — unlike the extended path.
        let (mut round_increment, mut round_mask, exp_offset) = if rounding_precision == 64 {
            (
                0x0000_0000_0000_0400_u64,
                0x0000_0000_0000_07FF_u64,
                0x3C00_i32,
            )
        } else {
            (
                0x0000_0080_0000_0000_u64,
                0x0000_00FF_FFFF_FFFF_u64,
                0x3F80_i32,
            )
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
        if (0x7FFE - exp_offset) < z_exp
            || (z_exp == 0x7FFE - exp_offset && z_sig0.wrapping_add(round_increment) < z_sig0)
        {
            float_raise(flag::OVERFLOW);
            if z_sig0 & round_mask != 0 {
                float_raise(flag::INEXACT);
            }
            if rounding_mode == RoundingMode::Zero
                || (z_sign && rounding_mode == RoundingMode::Up)
                || (!z_sign && rounding_mode == RoundingMode::Down)
            {
                return pack(z_sign, 0x7FFE - exp_offset, !round_mask);
            }
            return pack(z_sign, 0x7FFF, 0); // 68k infinity
        }
        if z_exp < exp_offset + 1 {
            // Below the format's minimum normal exponent → denormalize to the
            // single/double min, rounding the mantissa. (Tininess before
            // rounding, so underflow is unconditional here.)
            float_raise(flag::UNDERFLOW);
            z_sig0 = shift64_right_jamming(z_sig0, -(z_exp - (exp_offset + 1)));
            z_exp = exp_offset + 1;
            round_bits = z_sig0 & round_mask;
            if round_bits != 0 {
                float_raise(flag::INEXACT);
            }
            z_sig0 = z_sig0.wrapping_add(round_increment);
            round_increment = round_mask + 1;
            if round_nearest_even && round_bits << 1 == round_increment {
                round_mask |= round_increment;
            }
            z_sig0 &= !round_mask;
            return pack(z_sign, z_exp, z_sig0);
        }
        if round_bits != 0 {
            float_raise(flag::INEXACT);
        }
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
    if (z_exp as u32) >= 0x7FFE {
        if 0x7FFE < z_exp || (z_exp == 0x7FFE && z_sig0 == 0xFFFF_FFFF_FFFF_FFFF && increment) {
            return overflow(0, z_sig0, z_sig1);
        }
        if z_exp < 0 {
            // The 68881/2 always detects tininess before rounding: any result
            // with exponent < 0 underflows.
            float_raise(flag::UNDERFLOW);
            let (s0, s1) = shift64_extra_right_jamming(z_sig0, z_sig1, -z_exp);
            z_sig0 = s0;
            let z_sig1 = s1;
            z_exp = 0;
            if z_sig1 != 0 {
                float_raise(flag::INEXACT);
            }
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
                // 68k: no `if zSig0<0 zExp=1` renormalization.
            }
            return pack(z_sign, z_exp, z_sig0);
        }
    }
    if z_sig1 != 0 {
        float_raise(flag::INEXACT);
    }
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

/// `roundSigAndPackFloatx80` (SOFTFLOAT_68K): the rounding tail used by
/// FSGLMUL/FSGLDIV. A simpler single/double-precision rounder than
/// [`round_and_pack_floatx80`] — extended precision delegates straight back to
/// it. The 68881/2 always detects tininess before rounding, so any result with
/// exponent < 0 underflows.
#[must_use]
fn round_sig_and_pack_floatx80(
    rounding_precision: i32,
    rounding_mode: RoundingMode,
    z_sign: bool,
    mut z_exp: i32,
    mut z_sig0: u64,
    z_sig1: u64,
) -> FpReg {
    let round_nearest_even = rounding_mode == RoundingMode::NearestEven;
    let (mut round_increment, mut round_mask) = if rounding_precision == 32 {
        (0x0000_0080_0000_0000_u64, 0x0000_00FF_FFFF_FFFF_u64)
    } else if rounding_precision == 64 {
        (0x0000_0000_0000_0400_u64, 0x0000_0000_0000_07FF_u64)
    } else {
        return round_and_pack_floatx80(80, rounding_mode, z_sign, z_exp, z_sig0, z_sig1);
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
    if (z_exp as u32) >= 0x7FFE {
        if 0x7FFE < z_exp || (z_exp == 0x7FFE && z_sig0.wrapping_add(round_increment) < z_sig0) {
            float_raise(flag::OVERFLOW);
            if z_sig0 & round_mask != 0 {
                float_raise(flag::INEXACT);
            }
            if rounding_mode == RoundingMode::Zero
                || (z_sign && rounding_mode == RoundingMode::Up)
                || (!z_sign && rounding_mode == RoundingMode::Down)
            {
                return pack(z_sign, 0x7FFE, 0xFFFF_FFFF_FFFF_FFFF);
            }
            return pack(z_sign, 0x7FFF, 0); // 68k infinity
        }
        if z_exp < 0 {
            float_raise(flag::UNDERFLOW);
            z_sig0 = shift64_right_jamming(z_sig0, -z_exp);
            z_exp = 0;
            round_bits = z_sig0 & round_mask;
            if round_bits != 0 {
                float_raise(flag::INEXACT);
            }
            z_sig0 = z_sig0.wrapping_add(round_increment);
            if round_nearest_even && round_bits == round_increment {
                round_mask |= round_increment << 1;
            }
            z_sig0 &= !round_mask;
            return pack(z_sign, z_exp, z_sig0);
        }
    }
    if round_bits != 0 {
        float_raise(flag::INEXACT);
    }
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
    let mut a_exp = exp(a);
    let mut b_sig = frac(b);
    let mut b_exp = exp(b);
    // 68k (SOFTFLOAT_68K): normalize denormal operands up front; a zero
    // operand takes the sentinel exponent −64 so the magnitude comparison
    // routes it through the zero special-cases below.
    if a_exp == 0 {
        if a_sig == 0 {
            a_exp = -64;
        } else {
            let (e, s) = normalize_floatx80_subnormal(a_sig);
            a_exp = e;
            a_sig = s;
        }
    }
    if b_exp == 0 {
        if b_sig == 0 {
            b_exp = -64;
        } else {
            let (e, s) = normalize_floatx80_subnormal(b_sig);
            b_exp = e;
            b_sig = s;
        }
    }
    let exp_diff = a_exp - b_exp;

    let z_exp;
    let z_sig0;
    let z_sig1;
    if 0 < exp_diff {
        if a_exp == 0x7FFF {
            if a_sig.wrapping_shl(1) != 0 {
                return propagate_floatx80_nan(a, b);
            }
            return a; // 68881/2: inf_clear_intbit clear → passthrough unchanged
        }
        let (bs, s1) = shift64_extra_right_jamming(b_sig, 0, exp_diff);
        b_sig = bs;
        z_sig1 = s1;
        z_exp = a_exp;
    } else if exp_diff < 0 {
        if b_exp == 0x7FFF {
            if b_sig.wrapping_shl(1) != 0 {
                return propagate_floatx80_nan(a, b);
            }
            // 68881/2: inf_clear_intbit clear → keep b's significand.
            return pack(z_sign, 0x7FFF, b_sig);
        }
        let (as_, s1) = shift64_extra_right_jamming(a_sig, 0, -exp_diff);
        a_sig = as_;
        z_sig1 = s1;
        z_exp = b_exp;
    } else {
        // exp_diff == 0
        if a_exp == 0x7FFF {
            if (a_sig | b_sig).wrapping_shl(1) != 0 {
                return propagate_floatx80_nan(a, b);
            }
            // 68881/2: faddsub_swap_inf set → ∞ + ∞ returns operand B.
            return b;
        }
        z_sig1 = 0;
        z_sig0 = a_sig.wrapping_add(b_sig);
        if a_sig == 0 && b_sig == 0 {
            return pack(z_sign, 0, 0);
        }
        if a_sig == 0 || b_sig == 0 {
            return round_and_pack_floatx80(precision, mode, z_sign, a_exp, z_sig0, z_sig1);
        }
        // Two normalized significands always carry; shift right one place.
        return add_shift_right1_and_pack(precision, mode, z_sign, a_exp, z_sig0, z_sig1);
    }
    z_sig0 = a_sig.wrapping_add(b_sig);
    if (z_sig0 as i64) < 0 {
        return round_and_pack_floatx80(precision, mode, z_sign, z_exp, z_sig0, z_sig1);
    }
    add_shift_right1_and_pack(precision, mode, z_sign, z_exp, z_sig0, z_sig1)
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
    let a_exp = exp(a);
    let mut b_sig = frac(b);
    let b_exp = exp(b);
    // 68k (SOFTFLOAT_68K): no `bExp==0 → --expDiff` / `aExp==0 → ++expDiff` /
    // `aExp==0 → aExp=bExp=1` adjustments; denormal operands flow straight
    // through `normalizeRoundAndPackFloatx80`.
    let exp_diff = a_exp - b_exp;

    if 0 < exp_diff {
        // aExpBigger
        if a_exp == 0x7FFF {
            if a_sig.wrapping_shl(1) != 0 {
                return propagate_floatx80_nan(a, b);
            }
            // 68881/2: inf_clear_intbit is clear, so passthrough is unchanged.
            return a;
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
            // 68881/2: inf_clear_intbit clear → keep b's significand.
            return pack(!z_sign, 0x7FFF, b_sig);
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
        float_raise(flag::INVALID); // inf − inf
        return DEFAULT_NAN;
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

// --- FMOVE / FABS / FNEG, precision-aware (round the source to the rounding
// precision; identity at extended precision for finite operands). ---

/// `floatx80_move` (FMOVE): round `a` to `precision`.
#[must_use]
pub fn floatx80_move(precision: i32, mode: RoundingMode, a: FpReg) -> FpReg {
    let a_sig = frac(a);
    let a_exp = exp(a);
    let a_sign = sign(a);
    if a_exp == 0x7FFF {
        if a_sig.wrapping_shl(1) != 0 {
            return propagate_floatx80_nan_one_arg(a);
        }
        return a; // 68881/2: ∞ passthrough unchanged
    }
    if a_exp == 0 {
        if a_sig == 0 {
            return a;
        }
        return normalize_round_and_pack_floatx80(precision, mode, a_sign, a_exp, a_sig, 0);
    }
    round_and_pack_floatx80(precision, mode, a_sign, a_exp, a_sig, 0)
}

/// `floatx80_abs` (FABS): `|a|`, rounded to `precision`.
#[must_use]
pub fn floatx80_abs(precision: i32, mode: RoundingMode, a: FpReg) -> FpReg {
    let mut a_sig = frac(a);
    let mut a_exp = exp(a);
    if a_exp == 0x7FFF {
        if a_sig.wrapping_shl(1) != 0 {
            return propagate_floatx80_nan_one_arg(a);
        }
        return pack(false, a_exp, a_sig); // +∞
    }
    if a_exp == 0 {
        if a_sig == 0 {
            return pack(false, 0, 0);
        }
        let (e, s) = normalize_floatx80_subnormal(a_sig);
        a_exp = e;
        a_sig = s;
    }
    round_and_pack_floatx80(precision, mode, false, a_exp, a_sig, 0)
}

/// `floatx80_neg` (FNEG): `−a`, rounded to `precision`.
#[must_use]
pub fn floatx80_neg(precision: i32, mode: RoundingMode, a: FpReg) -> FpReg {
    let mut a_sig = frac(a);
    let mut a_exp = exp(a);
    let a_sign = sign(a);
    if a_exp == 0x7FFF {
        if a_sig.wrapping_shl(1) != 0 {
            return propagate_floatx80_nan_one_arg(a);
        }
        return pack(!a_sign, a_exp, a_sig);
    }
    let neg = !a_sign;
    if a_exp == 0 {
        if a_sig == 0 {
            return pack(neg, 0, 0);
        }
        let (e, s) = normalize_floatx80_subnormal(a_sig);
        a_exp = e;
        a_sig = s;
    }
    round_and_pack_floatx80(precision, mode, neg, a_exp, a_sig, 0)
}

// --- Multiplication / division (softfloat.c) ---

/// The default extended-precision NaN created by the FPCP for an invalid
/// operation. The MC68881/2 user's manual (§3.2.5) specifies an all-ones
/// mantissa; the silicon — and the silicon-validated WinUAE/Previous
/// SoftFloat — produces a **clear** sign bit, i.e. `$7FFF_FFFFFFFFFFFFFFFF`.
/// This deliberately differs from Berkeley/Musashi SoftFloat's generic
/// `$FFFF_…` (sign set), which is not what a real 68881/2 returns.
const DEFAULT_NAN: FpReg = FpReg::new(0x7FFF, 0xFFFF_FFFF_FFFF_FFFF);

/// `mul64To128`: full 64×64→128 product as (high, low). The exact 128-bit
/// product is bit-identical to SoftFloat's 32-bit-limb decomposition.
#[must_use]
fn mul64_to_128(a: u64, b: u64) -> (u64, u64) {
    let p = u128::from(a) * u128::from(b);
    ((p >> 64) as u64, p as u64)
}

/// `add128`: add the 128-bit values (`a0`,`a1`) and (`b0`,`b1`) modulo
/// 2^128.
#[must_use]
fn add128(a0: u64, a1: u64, b0: u64, b1: u64) -> (u64, u64) {
    let z1 = a1.wrapping_add(b1);
    let z0 = a0.wrapping_add(b0).wrapping_add(u64::from(z1 < a1));
    (z0, z1)
}

/// `eq128`: the 128-bit values (`a0`,`a1`) and (`b0`,`b1`) are equal.
#[must_use]
fn eq128(a0: u64, a1: u64, b0: u64, b1: u64) -> bool {
    a0 == b0 && a1 == b1
}

/// `le128`: (`a0`,`a1`) ≤ (`b0`,`b1`) as unsigned 128-bit values.
#[must_use]
fn le128(a0: u64, a1: u64, b0: u64, b1: u64) -> bool {
    a0 < b0 || (a0 == b0 && a1 <= b1)
}

/// `lt128`: (`a0`,`a1`) < (`b0`,`b1`) as unsigned 128-bit values.
#[must_use]
fn lt128(a0: u64, a1: u64, b0: u64, b1: u64) -> bool {
    a0 < b0 || (a0 == b0 && a1 < b1)
}

/// `shift128Right`: shift the 128-bit value (`a0`,`a1`) right by `count`,
/// dropping bits shifted off (no jamming). Faithful to SoftFloat — its
/// `count >= 64` arm collapses to zero (only `count == 1` is exercised by
/// `floatx80_div`).
#[must_use]
fn shift128_right(a0: u64, a1: u64, count: i32) -> (u64, u64) {
    if count == 0 {
        (a0, a1)
    } else if count < 64 {
        let c = count as u32;
        let neg = ((-count) & 63) as u32;
        ((a0 >> c), (a0 << neg) | (a1 >> c))
    } else {
        (0, 0)
    }
}

/// `estimateDiv128To64`: estimate the quotient of the 128-bit dividend
/// (`a0`,`a1`) by the 64-bit divisor `b` (which must be normalized, i.e.
/// have its MSB set). The estimate is exact or one too large.
#[must_use]
fn estimate_div128_to_64(a0: u64, a1: u64, b: u64) -> u64 {
    if b <= a0 {
        return 0xFFFF_FFFF_FFFF_FFFF;
    }
    let b0 = b >> 32;
    let mut z = if b0 << 32 <= a0 {
        0xFFFF_FFFF_0000_0000
    } else {
        (a0 / b0).wrapping_shl(32)
    };
    let (term0, term1) = mul64_to_128(b, z);
    let (mut rem0, mut rem1) = sub128(a0, a1, term0, term1);
    while (rem0 as i64) < 0 {
        z = z.wrapping_sub(0x1_0000_0000);
        let b1 = b.wrapping_shl(32);
        let r = add128(rem0, rem1, b0, b1);
        rem0 = r.0;
        rem1 = r.1;
    }
    let rem0 = (rem0 << 32) | (rem1 >> 32);
    z |= if b0 << 32 <= rem0 {
        0xFFFF_FFFF
    } else {
        rem0 / b0
    };
    z
}

/// `floatx80_mul`: multiply two extended-precision values.
#[must_use]
pub fn floatx80_mul(precision: i32, mode: RoundingMode, a: FpReg, b: FpReg) -> FpReg {
    let mut a_sig = frac(a);
    let mut a_exp = exp(a);
    let a_sign = sign(a);
    let mut b_sig = frac(b);
    let mut b_exp = exp(b);
    let b_sign = sign(b);
    let z_sign = a_sign ^ b_sign;

    if a_exp == 0x7FFF {
        if a_sig.wrapping_shl(1) != 0 || (b_exp == 0x7FFF && b_sig.wrapping_shl(1) != 0) {
            return propagate_floatx80_nan(a, b);
        }
        if (b_exp as u64 | b_sig) == 0 {
            float_raise(flag::INVALID); // inf × 0
            return DEFAULT_NAN;
        }
        // 68881/2: inf_clear_intbit clear → keep a's significand.
        return pack(z_sign, 0x7FFF, a_sig);
    }
    if b_exp == 0x7FFF {
        if b_sig.wrapping_shl(1) != 0 {
            return propagate_floatx80_nan(a, b);
        }
        if (a_exp as u64 | a_sig) == 0 {
            float_raise(flag::INVALID); // 0 × inf
            return DEFAULT_NAN;
        }
        return pack(z_sign, 0x7FFF, b_sig);
    }
    if a_exp == 0 {
        if a_sig == 0 {
            return pack(z_sign, 0, 0);
        }
        let (e, s) = normalize_floatx80_subnormal(a_sig);
        a_exp = e;
        a_sig = s;
    }
    if b_exp == 0 {
        if b_sig == 0 {
            return pack(z_sign, 0, 0);
        }
        let (e, s) = normalize_floatx80_subnormal(b_sig);
        b_exp = e;
        b_sig = s;
    }
    let mut z_exp = a_exp + b_exp - 0x3FFE;
    let (mut z_sig0, mut z_sig1) = mul64_to_128(a_sig, b_sig);
    if (z_sig0 as i64) > 0 {
        let (s0, s1) = short_shift128_left(z_sig0, z_sig1, 1);
        z_sig0 = s0;
        z_sig1 = s1;
        z_exp -= 1;
    }
    round_and_pack_floatx80(precision, mode, z_sign, z_exp, z_sig0, z_sig1)
}

/// `floatx80_sglmul` (FSGLMUL): multiply with both operands first truncated to
/// single-precision significands and the product rounded to single precision
/// (kept in extended format). Distinct from `floatx80_mul` at single precision,
/// which does not pre-truncate the operands.
#[must_use]
pub fn floatx80_sglmul(mode: RoundingMode, a: FpReg, b: FpReg) -> FpReg {
    let mut a_sig = frac(a);
    let mut a_exp = exp(a);
    let a_sign = sign(a);
    let mut b_sig = frac(b);
    let mut b_exp = exp(b);
    let b_sign = sign(b);
    let z_sign = a_sign ^ b_sign;

    if a_exp == 0x7FFF {
        if a_sig.wrapping_shl(1) != 0 || (b_exp == 0x7FFF && b_sig.wrapping_shl(1) != 0) {
            return propagate_floatx80_nan(a, b);
        }
        if (b_exp as u64 | b_sig) == 0 {
            float_raise(flag::INVALID); // inf × 0
            return DEFAULT_NAN;
        }
        return pack(z_sign, 0x7FFF, a_sig);
    }
    if b_exp == 0x7FFF {
        if b_sig.wrapping_shl(1) != 0 {
            return propagate_floatx80_nan(a, b);
        }
        if (a_exp as u64 | a_sig) == 0 {
            float_raise(flag::INVALID); // 0 × inf
            return DEFAULT_NAN;
        }
        return pack(z_sign, 0x7FFF, b_sig);
    }
    if a_exp == 0 {
        if a_sig == 0 {
            return pack(z_sign, 0, 0);
        }
        let (e, s) = normalize_floatx80_subnormal(a_sig);
        a_exp = e;
        a_sig = s;
    }
    if b_exp == 0 {
        if b_sig == 0 {
            return pack(z_sign, 0, 0);
        }
        let (e, s) = normalize_floatx80_subnormal(b_sig);
        b_exp = e;
        b_sig = s;
    }
    // Truncate each operand's significand to single precision (24 bits).
    a_sig &= 0xFFFF_FF00_0000_0000;
    b_sig &= 0xFFFF_FF00_0000_0000;
    let mut z_exp = a_exp + b_exp - 0x3FFE;
    let (mut z_sig0, mut z_sig1) = mul64_to_128(a_sig, b_sig);
    if (z_sig0 as i64) > 0 {
        let (s0, s1) = short_shift128_left(z_sig0, z_sig1, 1);
        z_sig0 = s0;
        z_sig1 = s1;
        z_exp -= 1;
    }
    round_sig_and_pack_floatx80(32, mode, z_sign, z_exp, z_sig0, z_sig1)
}

/// Outcome of the shared division core: either a finished special-case value,
/// or the unrounded quotient significand to feed a precision-specific rounder.
enum DivSigs {
    Done(FpReg),
    Round(bool, i32, u64, u64),
}

/// The division special-cases + significand estimate, shared by
/// [`floatx80_div`] (extended rounding) and [`floatx80_sgldiv`] (single).
fn floatx80_div_sigs(a: FpReg, b: FpReg) -> DivSigs {
    let mut a_sig = frac(a);
    let mut a_exp = exp(a);
    let a_sign = sign(a);
    let mut b_sig = frac(b);
    let mut b_exp = exp(b);
    let b_sign = sign(b);
    let z_sign = a_sign ^ b_sign;

    if a_exp == 0x7FFF {
        if a_sig.wrapping_shl(1) != 0 {
            return DivSigs::Done(propagate_floatx80_nan(a, b));
        }
        if b_exp == 0x7FFF {
            if b_sig.wrapping_shl(1) != 0 {
                return DivSigs::Done(propagate_floatx80_nan(a, b));
            }
            float_raise(flag::INVALID); // inf / inf
            return DivSigs::Done(DEFAULT_NAN);
        }
        // 68881/2: inf_clear_intbit clear → keep a's significand.
        return DivSigs::Done(pack(z_sign, 0x7FFF, a_sig)); // inf / finite
    }
    if b_exp == 0x7FFF {
        if b_sig.wrapping_shl(1) != 0 {
            return DivSigs::Done(propagate_floatx80_nan(a, b));
        }
        return DivSigs::Done(pack(z_sign, 0, 0)); // finite / inf = 0
    }
    if b_exp == 0 {
        if b_sig == 0 {
            if (a_exp as u64 | a_sig) == 0 {
                float_raise(flag::INVALID); // 0 / 0
                return DivSigs::Done(DEFAULT_NAN);
            }
            float_raise(flag::DIVBYZERO);
            return DivSigs::Done(pack(z_sign, 0x7FFF, 0)); // x / 0 = ∞ (int bit clear)
        }
        let (e, s) = normalize_floatx80_subnormal(b_sig);
        b_exp = e;
        b_sig = s;
    }
    if a_exp == 0 {
        if a_sig == 0 {
            return DivSigs::Done(pack(z_sign, 0, 0));
        }
        let (e, s) = normalize_floatx80_subnormal(a_sig);
        a_exp = e;
        a_sig = s;
    }
    let mut z_exp = a_exp - b_exp + 0x3FFE;
    let mut rem1 = 0u64;
    if b_sig <= a_sig {
        let (s0, s1) = shift128_right(a_sig, 0, 1);
        a_sig = s0;
        rem1 = s1;
        z_exp += 1;
    }
    let mut z_sig0 = estimate_div128_to_64(a_sig, rem1, b_sig);
    let (term0, term1) = mul64_to_128(b_sig, z_sig0);
    let (mut rem0, mut rem1) = sub128(a_sig, rem1, term0, term1);
    while (rem0 as i64) < 0 {
        z_sig0 = z_sig0.wrapping_sub(1);
        let r = add128(rem0, rem1, 0, b_sig);
        rem0 = r.0;
        rem1 = r.1;
    }
    let mut z_sig1 = estimate_div128_to_64(rem1, 0, b_sig);
    if z_sig1.wrapping_shl(1) <= 8 {
        let (term1, term2) = mul64_to_128(b_sig, z_sig1);
        let (r1, mut rem2) = sub128(rem1, 0, term1, term2);
        rem1 = r1;
        while (rem1 as i64) < 0 {
            z_sig1 = z_sig1.wrapping_sub(1);
            let r = add128(rem1, rem2, 0, b_sig);
            rem1 = r.0;
            rem2 = r.1;
        }
        z_sig1 |= u64::from((rem1 | rem2) != 0);
    }
    DivSigs::Round(z_sign, z_exp, z_sig0, z_sig1)
}

/// `floatx80_div`: divide `a` by `b` at extended precision.
#[must_use]
pub fn floatx80_div(precision: i32, mode: RoundingMode, a: FpReg, b: FpReg) -> FpReg {
    match floatx80_div_sigs(a, b) {
        DivSigs::Done(z) => z,
        DivSigs::Round(s, e, s0, s1) => round_and_pack_floatx80(precision, mode, s, e, s0, s1),
    }
}

/// `floatx80_sgldiv` (FSGLDIV): divide `a` by `b` with the quotient rounded to
/// single precision (kept in extended format). The full extended division is
/// performed; only the final rounding differs from [`floatx80_div`].
#[must_use]
pub fn floatx80_sgldiv(mode: RoundingMode, a: FpReg, b: FpReg) -> FpReg {
    match floatx80_div_sigs(a, b) {
        DivSigs::Done(z) => z,
        DivSigs::Round(s, e, s0, s1) => round_sig_and_pack_floatx80(32, mode, s, e, s0, s1),
    }
}

// --- Square root (softfloat.c) ---

/// `sub192`: subtract the 192-bit value (`b0`,`b1`,`b2`) from
/// (`a0`,`a1`,`a2`) modulo 2^192.
#[must_use]
fn sub192(a0: u64, a1: u64, a2: u64, b0: u64, b1: u64, b2: u64) -> (u64, u64, u64) {
    let z2 = a2.wrapping_sub(b2);
    let borrow1 = u64::from(a2 < b2);
    let z1 = a1.wrapping_sub(b1);
    let borrow0 = u64::from(a1 < b1);
    let mut z0 = a0.wrapping_sub(b0);
    z0 = z0.wrapping_sub(u64::from(z1 < borrow1));
    let z1 = z1.wrapping_sub(borrow1);
    z0 = z0.wrapping_sub(borrow0);
    (z0, z1, z2)
}

/// `add192`: add the 192-bit values (`a0`,`a1`,`a2`) and (`b0`,`b1`,`b2`)
/// modulo 2^192.
#[must_use]
fn add192(a0: u64, a1: u64, a2: u64, b0: u64, b1: u64, b2: u64) -> (u64, u64, u64) {
    let z2 = a2.wrapping_add(b2);
    let carry1 = u64::from(z2 < a2);
    let mut z1 = a1.wrapping_add(b1);
    let carry0 = u64::from(z1 < a1);
    let mut z0 = a0.wrapping_add(b0);
    z1 = z1.wrapping_add(carry1);
    z0 = z0.wrapping_add(u64::from(z1 < carry1));
    z0 = z0.wrapping_add(carry0);
    (z0, z1, z2)
}

/// `estimateSqrt32`: estimate the square root of a 32-bit fraction `a`
/// scaled by `aExp`'s parity, to ~30 bits. Used to seed the `floatx80`
/// Newton iteration. Faithful to SoftFloat's lookup-and-divide method.
#[must_use]
fn estimate_sqrt32(a_exp: i32, mut a: u32) -> u32 {
    const ODD: [u16; 16] = [
        0x0004, 0x0022, 0x005D, 0x00B1, 0x011D, 0x019F, 0x0236, 0x02E0, 0x039C, 0x0468, 0x0545,
        0x0631, 0x072B, 0x0832, 0x0946, 0x0A67,
    ];
    const EVEN: [u16; 16] = [
        0x0A2D, 0x08AF, 0x075A, 0x0629, 0x051A, 0x0429, 0x0356, 0x029E, 0x0200, 0x0179, 0x0109,
        0x00AF, 0x0068, 0x0034, 0x0012, 0x0002,
    ];
    let index = ((a >> 27) & 15) as usize;
    let z;
    if a_exp & 1 != 0 {
        let t = 0x4000_u32
            .wrapping_add(a >> 17)
            .wrapping_sub(u32::from(ODD[index]));
        z = (a / t).wrapping_shl(14).wrapping_add(t.wrapping_shl(15));
        a >>= 1;
    } else {
        let mut t = 0x8000_u32
            .wrapping_add(a >> 17)
            .wrapping_sub(u32::from(EVEN[index]));
        t = (a / t).wrapping_add(t);
        t = if 0x20000 <= t {
            0xFFFF_8000
        } else {
            t.wrapping_shl(15)
        };
        if t <= a {
            return ((a as i32) >> 1) as u32;
        }
        z = t;
    }
    (((u64::from(a) << 31) / u64::from(z)) as u32).wrapping_add(z >> 1)
}

/// `floatx80_sqrt`: extended-precision square root.
#[must_use]
pub fn floatx80_sqrt(precision: i32, mode: RoundingMode, a: FpReg) -> FpReg {
    let mut a_sig0 = frac(a);
    let mut a_exp = exp(a);
    let a_sign = sign(a);

    if a_exp == 0x7FFF {
        if a_sig0.wrapping_shl(1) != 0 {
            return propagate_floatx80_nan(a, a);
        }
        if !a_sign {
            return a; // +inf
        }
        float_raise(flag::INVALID); // sqrt(−inf)
        return DEFAULT_NAN;
    }
    if a_sign {
        if (a_exp as u64 | a_sig0) == 0 {
            return a; // sqrt(−0) = −0
        }
        float_raise(flag::INVALID); // sqrt(negative)
        return DEFAULT_NAN;
    }
    if a_exp == 0 {
        if a_sig0 == 0 {
            return pack(false, 0, 0); // sqrt(+0) = +0
        }
        let (e, s) = normalize_floatx80_subnormal(a_sig0);
        a_exp = e;
        a_sig0 = s;
    }
    let z_exp = ((a_exp - 0x3FFF) >> 1) + 0x3FFF;
    let mut z_sig0 = u64::from(estimate_sqrt32(a_exp, (a_sig0 >> 32) as u32));
    let (s0, a_sig1) = shift128_right(a_sig0, 0, 2 + (a_exp & 1));
    a_sig0 = s0;
    z_sig0 = estimate_div128_to_64(a_sig0, a_sig1, z_sig0 << 32).wrapping_add(z_sig0 << 30);
    let mut double_z_sig0 = z_sig0.wrapping_shl(1);
    let (term0, term1) = mul64_to_128(z_sig0, z_sig0);
    let (mut rem0, mut rem1) = sub128(a_sig0, a_sig1, term0, term1);
    while (rem0 as i64) < 0 {
        z_sig0 = z_sig0.wrapping_sub(1);
        double_z_sig0 = double_z_sig0.wrapping_sub(2);
        let r = add128(rem0, rem1, z_sig0 >> 63, double_z_sig0 | 1);
        rem0 = r.0;
        rem1 = r.1;
    }
    let mut z_sig1 = estimate_div128_to_64(rem1, 0, double_z_sig0);
    if (z_sig1 & 0x3FFF_FFFF_FFFF_FFFF) <= 5 {
        if z_sig1 == 0 {
            z_sig1 = 1;
        }
        let (term1, term2) = mul64_to_128(double_z_sig0, z_sig1);
        let (r1, mut rem2) = sub128(rem1, 0, term1, term2);
        rem1 = r1;
        let (term2b, term3) = mul64_to_128(z_sig1, z_sig1);
        let (r1b, r2, mut rem3) = sub192(rem1, rem2, 0, 0, term2b, term3);
        rem1 = r1b;
        rem2 = r2;
        while (rem1 as i64) < 0 {
            z_sig1 = z_sig1.wrapping_sub(1);
            let (t2, mut t3) = short_shift128_left(0, z_sig1, 1);
            t3 |= 1;
            let t2 = t2 | double_z_sig0;
            let r = add192(rem1, rem2, rem3, 0, t2, t3);
            rem1 = r.0;
            rem2 = r.1;
            rem3 = r.2;
        }
        z_sig1 |= u64::from((rem1 | rem2 | rem3) != 0);
    }
    let (s0b, z_sig1b) = short_shift128_left(0, z_sig1, 1);
    z_sig0 = s0b | double_z_sig0;
    round_and_pack_floatx80(precision, mode, false, z_exp, z_sig0, z_sig1b)
}

// --- Remainder / modulo (FREM / FMOD, with the FPSR quotient byte) ---

/// The result of FREM / FMOD: the reduced value plus the seven low quotient
/// bits and the quotient sign, which the core writes into the FPSR quotient
/// byte (bit 23 = `sign`, bits 22-16 = `quotient & 0x7F`).
#[derive(Clone, Copy)]
pub struct RemResult {
    pub value: FpReg,
    pub quotient: u64,
    pub sign: bool,
}

/// `floatx80_rem` (FREM): the IEEE remainder of `a` ÷ `b` — `a − n·b` where `n`
/// is the quotient rounded to the nearest integer.
#[must_use]
pub fn floatx80_rem(precision: i32, mode: RoundingMode, a: FpReg, b: FpReg) -> RemResult {
    let mut a_sig0 = frac(a);
    let mut a_exp = exp(a);
    let a_sign = sign(a);
    let mut b_sig = frac(b);
    let mut b_exp = exp(b);
    let b_sign = sign(b);

    let nan = |v: FpReg| RemResult {
        value: v,
        quotient: 0,
        sign: false,
    };

    if a_exp == 0x7FFF {
        if a_sig0.wrapping_shl(1) != 0 || (b_exp == 0x7FFF && b_sig.wrapping_shl(1) != 0) {
            return nan(propagate_floatx80_nan(a, b));
        }
        float_raise(flag::INVALID); // ∞ rem y
        return nan(DEFAULT_NAN);
    }
    if b_exp == 0x7FFF {
        if b_sig.wrapping_shl(1) != 0 {
            return nan(propagate_floatx80_nan(a, b));
        }
        // y rem ∞ = y.
        let value = normalize_round_and_pack_floatx80(precision, mode, a_sign, a_exp, a_sig0, 0);
        return RemResult {
            value,
            quotient: 0,
            sign: a_sign != b_sign,
        };
    }
    if b_exp == 0 {
        if b_sig == 0 {
            float_raise(flag::INVALID); // y rem 0
            return nan(DEFAULT_NAN);
        }
        let (e, s) = normalize_floatx80_subnormal(b_sig);
        b_exp = e;
        b_sig = s;
    }
    if a_exp == 0 {
        if a_sig0 == 0 {
            return RemResult {
                value: a,
                quotient: 0,
                sign: a_sign != b_sign,
            };
        }
        let (e, s) = normalize_floatx80_subnormal(a_sig0);
        a_exp = e;
        a_sig0 = s;
    }
    b_sig |= 0x8000_0000_0000_0000;
    let mut z_sign = a_sign;
    let mut exp_diff = a_exp - b_exp;
    let sign = a_sign != b_sign;
    let mut a_sig1 = 0u64;
    if exp_diff < 0 {
        if exp_diff < -1 {
            return RemResult {
                value: a,
                quotient: 0,
                sign,
            };
        }
        let (s0, s1) = shift128_right(a_sig0, 0, 1);
        a_sig0 = s0;
        a_sig1 = s1;
        exp_diff = 0;
    }
    let mut q_temp = u64::from(b_sig <= a_sig0);
    if q_temp != 0 {
        a_sig0 = a_sig0.wrapping_sub(b_sig);
    }
    let mut q = if exp_diff > 63 {
        0
    } else {
        q_temp.wrapping_shl(exp_diff as u32)
    };
    exp_diff -= 64;
    while 0 < exp_diff {
        q_temp = estimate_div128_to_64(a_sig0, a_sig1, b_sig);
        q_temp = q_temp.saturating_sub(2);
        let (term0, term1) = mul64_to_128(b_sig, q_temp);
        let (s0, s1) = sub128(a_sig0, a_sig1, term0, term1);
        let (s0b, s1b) = short_shift128_left(s0, s1, 62);
        a_sig0 = s0b;
        a_sig1 = s1b;
        q = if exp_diff > 63 {
            0
        } else {
            q_temp.wrapping_shl(exp_diff as u32)
        };
        exp_diff -= 62;
    }
    exp_diff += 64;
    let term0;
    let term1;
    if 0 < exp_diff {
        q_temp = estimate_div128_to_64(a_sig0, a_sig1, b_sig);
        q_temp = q_temp.saturating_sub(2);
        q_temp >>= 64 - exp_diff;
        let (t0, t1) = mul64_to_128(b_sig, q_temp << (64 - exp_diff));
        let (s0, s1) = sub128(a_sig0, a_sig1, t0, t1);
        a_sig0 = s0;
        a_sig1 = s1;
        let (bt0, bt1) = short_shift128_left(0, b_sig, 64 - exp_diff);
        while le128(bt0, bt1, a_sig0, a_sig1) {
            q_temp += 1;
            let (s0c, s1c) = sub128(a_sig0, a_sig1, bt0, bt1);
            a_sig0 = s0c;
            a_sig1 = s1c;
        }
        q = q.wrapping_add(q_temp);
        term0 = bt0;
        term1 = bt1;
    } else {
        term1 = 0;
        term0 = b_sig;
    }
    // Round the quotient to nearest: if the remainder is more than half of |b|
    // (or exactly half with an odd quotient), take the complement.
    let (alt0, alt1) = sub128(term0, term1, a_sig0, a_sig1);
    if lt128(alt0, alt1, a_sig0, a_sig1) || (eq128(alt0, alt1, a_sig0, a_sig1) && (q_temp & 1) != 0)
    {
        a_sig0 = alt0;
        a_sig1 = alt1;
        z_sign = !z_sign;
        q = q.wrapping_add(1);
    }
    let value = normalize_round_and_pack_floatx80(
        precision,
        mode,
        z_sign,
        b_exp + exp_diff,
        a_sig0,
        a_sig1,
    );
    RemResult {
        value,
        quotient: q,
        sign,
    }
}

/// `floatx80_mod` (FMOD): the modulo remainder of `a` ÷ `b` — like FREM but the
/// quotient is truncated toward zero rather than rounded to nearest.
#[must_use]
pub fn floatx80_mod(precision: i32, mode: RoundingMode, a: FpReg, b: FpReg) -> RemResult {
    let mut a_sig0 = frac(a);
    let mut a_exp = exp(a);
    let a_sign = sign(a);
    let mut b_sig = frac(b);
    let mut b_exp = exp(b);
    let b_sign = sign(b);

    let nan = |v: FpReg| RemResult {
        value: v,
        quotient: 0,
        sign: false,
    };

    if a_exp == 0x7FFF {
        if a_sig0.wrapping_shl(1) != 0 || (b_exp == 0x7FFF && b_sig.wrapping_shl(1) != 0) {
            return nan(propagate_floatx80_nan(a, b));
        }
        float_raise(flag::INVALID);
        return nan(DEFAULT_NAN);
    }
    if b_exp == 0x7FFF {
        if b_sig.wrapping_shl(1) != 0 {
            return nan(propagate_floatx80_nan(a, b));
        }
        let value = normalize_round_and_pack_floatx80(precision, mode, a_sign, a_exp, a_sig0, 0);
        return RemResult {
            value,
            quotient: 0,
            sign: a_sign != b_sign,
        };
    }
    if b_exp == 0 {
        if b_sig == 0 {
            float_raise(flag::INVALID);
            return nan(DEFAULT_NAN);
        }
        let (e, s) = normalize_floatx80_subnormal(b_sig);
        b_exp = e;
        b_sig = s;
    }
    if a_exp == 0 {
        if a_sig0 == 0 {
            return RemResult {
                value: a,
                quotient: 0,
                sign: a_sign != b_sign,
            };
        }
        let (e, s) = normalize_floatx80_subnormal(a_sig0);
        a_exp = e;
        a_sig0 = s;
    }
    b_sig |= 0x8000_0000_0000_0000;
    let z_sign = a_sign;
    let mut exp_diff = a_exp - b_exp;
    let sign = a_sign != b_sign;
    let mut a_sig1 = 0u64;
    if exp_diff < 0 {
        // |a| < |b| already — FMOD leaves a untouched (truncated quotient 0).
        let value = round_and_pack_floatx80(precision, mode, a_sign, a_exp, a_sig0, 0);
        return RemResult {
            value,
            quotient: 0,
            sign,
        };
    }
    let mut q_temp = u64::from(b_sig <= a_sig0);
    if q_temp != 0 {
        a_sig0 = a_sig0.wrapping_sub(b_sig);
    }
    let mut q = if exp_diff > 63 {
        0
    } else {
        q_temp.wrapping_shl(exp_diff as u32)
    };
    exp_diff -= 64;
    while 0 < exp_diff {
        q_temp = estimate_div128_to_64(a_sig0, a_sig1, b_sig);
        q_temp = q_temp.saturating_sub(2);
        let (term0, term1) = mul64_to_128(b_sig, q_temp);
        let (s0, s1) = sub128(a_sig0, a_sig1, term0, term1);
        let (s0b, s1b) = short_shift128_left(s0, s1, 62);
        a_sig0 = s0b;
        a_sig1 = s1b;
        q = if exp_diff > 63 {
            0
        } else {
            q_temp.wrapping_shl(exp_diff as u32)
        };
        exp_diff -= 62;
    }
    exp_diff += 64;
    if 0 < exp_diff {
        q_temp = estimate_div128_to_64(a_sig0, a_sig1, b_sig);
        q_temp = q_temp.saturating_sub(2);
        q_temp >>= 64 - exp_diff;
        let (t0, t1) = mul64_to_128(b_sig, q_temp << (64 - exp_diff));
        let (s0, s1) = sub128(a_sig0, a_sig1, t0, t1);
        a_sig0 = s0;
        a_sig1 = s1;
        let (bt0, bt1) = short_shift128_left(0, b_sig, 64 - exp_diff);
        while le128(bt0, bt1, a_sig0, a_sig1) {
            q_temp += 1;
            let (s0c, s1c) = sub128(a_sig0, a_sig1, bt0, bt1);
            a_sig0 = s0c;
            a_sig1 = s1c;
        }
        q = q.wrapping_add(q_temp);
    }
    let value = normalize_round_and_pack_floatx80(
        precision,
        mode,
        z_sign,
        b_exp + exp_diff,
        a_sig0,
        a_sig1,
    );
    RemResult {
        value,
        quotient: q,
        sign,
    }
}

// --- FPSP exponent / mantissa / scale (WinUAE softfloat, Andreas Grabher) ---
//
// Ported from WinUAE's `softfloat.cpp` (`floatx80_getman`/`getexp`/`scale`),
// the silicon-validated 68881/2 reference. All three are bit-exact against
// that source (the shared `floatx80` core now follows SOFTFLOAT_68K), validated
// by the C-diff harness `validation/run_fpsp.sh`.

/// `propagateFloatx80NaNOneArg`: quiet a single NaN operand (set the quiet
/// bit), raising invalid (SNAN) if it was signalling.
#[must_use]
pub(crate) fn propagate_floatx80_nan_one_arg(mut a: FpReg) -> FpReg {
    if is_signaling_nan(a) {
        raise_signaling_nan();
    }
    a.low |= 0x4000_0000_0000_0000;
    a
}

/// `floatx80_getman` (FGETMAN): the mantissa of `a` as a value in `[1, 2)`.
#[must_use]
pub fn floatx80_getman(a: FpReg) -> FpReg {
    let mut a_sig = frac(a);
    let a_exp = exp(a);
    let a_sign = sign(a);
    if a_exp == 0x7FFF {
        if a_sig.wrapping_shl(1) != 0 {
            return propagate_floatx80_nan_one_arg(a);
        }
        float_raise(flag::INVALID);
        return DEFAULT_NAN;
    }
    if a_exp == 0 {
        if a_sig == 0 {
            return pack(a_sign, 0, 0);
        }
        // Only the normalized significand matters; the exponent is replaced.
        let (_, s) = normalize_floatx80_subnormal(a_sig);
        a_sig = s;
    }
    pack(a_sign, 0x3FFF, a_sig)
}

/// `floatx80_getexp` (FGETEXP): the unbiased exponent of `a` as an integer
/// floating-point value.
#[must_use]
pub fn floatx80_getexp(a: FpReg) -> FpReg {
    let a_sig = frac(a);
    let mut a_exp = exp(a);
    let a_sign = sign(a);
    if a_exp == 0x7FFF {
        if a_sig.wrapping_shl(1) != 0 {
            return propagate_floatx80_nan_one_arg(a);
        }
        float_raise(flag::INVALID);
        return DEFAULT_NAN;
    }
    if a_exp == 0 {
        if a_sig == 0 {
            return pack(a_sign, 0, 0);
        }
        // Only the normalized exponent matters for the result.
        let (e, _) = normalize_floatx80_subnormal(a_sig);
        a_exp = e;
    }
    int32_to_floatx80(a_exp - 0x3FFF)
}

/// `floatx80_scale` (FSCALE): scale `a` by 2 raised to the truncated integer
/// value of `b`.
#[must_use]
pub fn floatx80_scale(precision: i32, mode: RoundingMode, a: FpReg, b: FpReg) -> FpReg {
    let mut a_sig = frac(a);
    let mut a_exp = exp(a);
    let a_sign = sign(a);
    let b_sig = frac(b);
    let b_exp = exp(b);
    let b_sign = sign(b);

    if b_exp == 0x7FFF {
        if b_sig.wrapping_shl(1) != 0 || (a_exp == 0x7FFF && a_sig.wrapping_shl(1) != 0) {
            return propagate_floatx80_nan(a, b);
        }
        float_raise(flag::INVALID);
        return DEFAULT_NAN;
    }
    if a_exp == 0x7FFF {
        if a_sig.wrapping_shl(1) != 0 {
            return propagate_floatx80_nan(a, b);
        }
        return a;
    }
    if a_exp == 0 {
        if a_sig == 0 {
            return pack(a_sign, 0, 0);
        }
        if b_exp < 0x3FFF {
            return normalize_round_and_pack_floatx80(precision, mode, a_sign, a_exp, a_sig, 0);
        }
        let (e, s) = normalize_floatx80_subnormal(a_sig);
        a_exp = e;
        a_sig = s;
    }
    if b_exp < 0x3FFF {
        return round_and_pack_floatx80(precision, mode, a_sign, a_exp, a_sig, 0);
    }
    if 0x400F < b_exp {
        a_exp = if b_sign { -0x6001 } else { 0xE000 };
        return round_and_pack_floatx80(precision, mode, a_sign, a_exp, a_sig, 0);
    }
    let shift_count = 0x403E - b_exp;
    let b_int = (b_sig.wrapping_shr(shift_count as u32)) as i32;
    a_exp = if b_sign { a_exp - b_int } else { a_exp + b_int };
    round_and_pack_floatx80(precision, mode, a_sign, a_exp, a_sig, 0)
}

// --- Conversion to 32-bit integer (softfloat.c) ---

/// `roundAndPackInt32`: round the unsigned 64-bit magnitude `abs_z` (with
/// 7 guard bits in its low bits) to a signed 32-bit integer under
/// `mode`, saturating on overflow. Value-only; the inexact/invalid flags
/// are deferred (TODO(FPSR)).
#[must_use]
fn round_and_pack_int32(mode: RoundingMode, z_sign: bool, abs_z: u64) -> i32 {
    let round_nearest_even = mode == RoundingMode::NearestEven;
    let mut round_increment: u64 = 0x40;
    if !round_nearest_even {
        if mode == RoundingMode::Zero {
            round_increment = 0;
        } else {
            round_increment = 0x7F;
            if z_sign {
                if mode == RoundingMode::Up {
                    round_increment = 0;
                }
            } else if mode == RoundingMode::Down {
                round_increment = 0;
            }
        }
    }
    let round_bits = abs_z & 0x7F;
    let mut abs_z = abs_z.wrapping_add(round_increment) >> 7;
    abs_z &= !u64::from((round_bits ^ 0x40) == 0 && round_nearest_even);
    let mut z = abs_z as i32;
    if z_sign {
        z = z.wrapping_neg();
    }
    if (abs_z >> 32) != 0 || (z != 0 && ((z < 0) != z_sign)) {
        float_raise(flag::INVALID);
        return if z_sign { i32::MIN } else { i32::MAX };
    }
    if round_bits != 0 {
        float_raise(flag::INEXACT);
    }
    z
}

/// `floatx80_to_int32`: convert to a signed 32-bit integer, rounding per
/// `mode` (used by FINT / FMOVE.L Fpn,Dn).
#[must_use]
pub fn floatx80_to_int32(mode: RoundingMode, a: FpReg) -> i32 {
    let mut a_sig = frac(a);
    let a_exp = exp(a);
    let a_sign = sign(a);
    if a_exp == 0x7FFF {
        if a_sig.wrapping_shl(1) != 0 {
            // 68k: NaN → quiet it (signaling raises invalid inside propagate;
            // an already-quiet input raises invalid here), then return the high
            // 32 bits of the resulting mantissa.
            let q = propagate_floatx80_nan_one_arg(a);
            if q.low == a_sig {
                float_raise(flag::INVALID);
            }
            return (q.low >> 32) as i32;
        }
        float_raise(flag::INVALID); // infinity
        return if a_sign { i32::MIN } else { i32::MAX };
    }
    let mut shift_count = 0x4037 - a_exp;
    if shift_count <= 0 {
        shift_count = 1;
    }
    a_sig = shift64_right_jamming(a_sig, shift_count);
    round_and_pack_int32(mode, a_sign, a_sig)
}

/// `floatx80_to_int32_round_to_zero`: convert to a signed 32-bit integer,
/// truncating toward zero regardless of the FPCR mode (FINTRZ / the
/// integer-part ops). Saturates on overflow.
#[must_use]
pub fn floatx80_to_int32_round_to_zero(a: FpReg) -> i32 {
    let a_sig = frac(a);
    let a_exp = exp(a);
    let mut a_sign = sign(a);
    if 0x401E < a_exp {
        if a_exp == 0x7FFF && a_sig.wrapping_shl(1) != 0 {
            a_sign = false;
        }
        float_raise(flag::INVALID);
        return if a_sign { i32::MIN } else { i32::MAX };
    } else if a_exp < 0x3FFF {
        if a_exp != 0 || a_sig != 0 {
            float_raise(flag::INEXACT);
        }
        return 0;
    }
    let shift_count = 0x403E - a_exp;
    let mut z = (a_sig >> shift_count) as i32;
    if a_sign {
        z = z.wrapping_neg();
    }
    if (z < 0) != a_sign {
        float_raise(flag::INVALID);
        return if a_sign { i32::MIN } else { i32::MAX };
    }
    // Inexact when low bits were dropped (C compares the re-expanded value
    // against the original significand).
    if (a_sig >> shift_count) << shift_count != a_sig {
        float_raise(flag::INEXACT);
    }
    z
}

// --- IEEE single / double → extended (softfloat.c) ---

/// `commonNaNToFloatx80` fused with the float32/64→commonNaN extraction:
/// build the extended-precision NaN from a sign and the canonical NaN's
/// high 64 fraction bits. (The signaling-NaN `float_raise(invalid)` is a
/// deferred side effect — TODO(FPSR).)
#[must_use]
fn common_nan_to_floatx80(sign: bool, common_high: u64) -> FpReg {
    // 68k: quiet via bit 62 only; the integer bit (63) comes from the payload.
    let low = 0x4000_0000_0000_0000 | (common_high >> 1);
    let high = ((sign as u16) << 15) | 0x7FFF;
    FpReg::new(high, low)
}

/// `normalizeFloat32Subnormal`: normalize a 23-bit subnormal significand.
#[must_use]
fn normalize_float32_subnormal(a_sig: u32) -> (i32, u32) {
    let shift_count = a_sig.leading_zeros() as i32 - 8;
    (1 - shift_count, a_sig << shift_count)
}

/// `float32_to_floatx80`: widen an IEEE single (raw 32-bit bit pattern) to
/// extended precision. Exact — single fits in extended with no rounding.
#[must_use]
pub fn float32_to_floatx80(a: u32) -> FpReg {
    let mut a_sig = a & 0x007F_FFFF;
    let mut a_exp = ((a >> 23) & 0xFF) as i32;
    let a_sign = (a >> 31) != 0;
    if a_exp == 0xFF {
        if a_sig != 0 {
            if float32_is_signaling_nan(a) {
                raise_signaling_nan();
            }
            return common_nan_to_floatx80(a_sign, u64::from(a) << 41);
        }
        return pack(a_sign, 0x7FFF, 0); // 68k infinity: integer bit clear
    }
    if a_exp == 0 {
        if a_sig == 0 {
            return pack(a_sign, 0, 0);
        }
        let (e, s) = normalize_float32_subnormal(a_sig);
        a_exp = e;
        a_sig = s;
    }
    a_sig |= 0x0080_0000;
    pack(a_sign, a_exp + 0x3F80, u64::from(a_sig) << 40)
}

/// `normalizeFloat64Subnormal`: normalize a 52-bit subnormal significand.
#[must_use]
fn normalize_float64_subnormal(a_sig: u64) -> (i32, u64) {
    let shift_count = a_sig.leading_zeros() as i32 - 11;
    (1 - shift_count, a_sig << shift_count)
}

/// `float64_to_floatx80`: widen an IEEE double (raw 64-bit bit pattern) to
/// extended precision. Exact — double fits in extended with no rounding.
#[must_use]
pub fn float64_to_floatx80(a: u64) -> FpReg {
    let mut a_sig = a & 0x000F_FFFF_FFFF_FFFF;
    let mut a_exp = ((a >> 52) & 0x7FF) as i32;
    let a_sign = (a >> 63) != 0;
    if a_exp == 0x7FF {
        if a_sig != 0 {
            if float64_is_signaling_nan(a) {
                raise_signaling_nan();
            }
            return common_nan_to_floatx80(a_sign, a << 12);
        }
        return pack(a_sign, 0x7FFF, 0); // 68k infinity: integer bit clear
    }
    if a_exp == 0 {
        if a_sig == 0 {
            return pack(a_sign, 0, 0);
        }
        let (e, s) = normalize_float64_subnormal(a_sig);
        a_exp = e;
        a_sig = s;
    }
    pack(
        a_sign,
        a_exp + 0x3C00,
        (a_sig | 0x0010_0000_0000_0000) << 11,
    )
}

// --- extended → IEEE single / double (softfloat.c) ---

/// `shift32RightJamming`: 32-bit right shift with a sticky bit.
#[must_use]
fn shift32_right_jamming(a: u32, count: i32) -> u32 {
    if count == 0 {
        a
    } else if count < 32 {
        let c = count as u32;
        (a >> c) | u32::from(a.wrapping_shl(c.wrapping_neg() & 31) != 0)
    } else {
        u32::from(a != 0)
    }
}

/// `packFloat32`: assemble a single-precision bit pattern.
#[must_use]
fn pack_float32(z_sign: bool, z_exp: i32, z_sig: u32) -> u32 {
    ((z_sign as u32) << 31)
        .wrapping_add((z_exp as u32) << 23)
        .wrapping_add(z_sig)
}

/// `packFloat64`: assemble a double-precision bit pattern.
#[must_use]
fn pack_float64(z_sign: bool, z_exp: i32, z_sig: u64) -> u64 {
    ((z_sign as u64) << 63)
        .wrapping_add((z_exp as u64) << 52)
        .wrapping_add(z_sig)
}

/// The extended-precision NaN `a` reduced to a single-precision NaN
/// (fused `commonNaNToFloat32(floatx80ToCommonNaN(a))`).
#[must_use]
fn floatx80_nan_to_float32(a: FpReg) -> u32 {
    if is_signaling_nan(a) {
        raise_signaling_nan();
    }
    let sign = u32::from((a.high >> 15) & 1);
    let common_high = a.low << 1;
    (sign << 31) | 0x7FC0_0000 | (common_high >> 41) as u32
}

/// The extended-precision NaN `a` reduced to a double-precision NaN.
#[must_use]
fn floatx80_nan_to_float64(a: FpReg) -> u64 {
    if is_signaling_nan(a) {
        raise_signaling_nan();
    }
    let sign = u64::from((a.high >> 15) & 1);
    let common_high = a.low << 1;
    (sign << 63) | 0x7FF8_0000_0000_0000 | (common_high >> 12)
}

/// `roundAndPackFloat32`: round a 32-bit significand (7 guard bits) to
/// single precision under `mode`. Value-only (FPSR side effects deferred).
#[must_use]
fn round_and_pack_float32(mode: RoundingMode, z_sign: bool, mut z_exp: i32, mut z_sig: u32) -> u32 {
    let round_nearest_even = mode == RoundingMode::NearestEven;
    let mut round_increment: u32 = 0x40;
    if !round_nearest_even {
        if mode == RoundingMode::Zero {
            round_increment = 0;
        } else {
            round_increment = 0x7F;
            if z_sign {
                if mode == RoundingMode::Up {
                    round_increment = 0;
                }
            } else if mode == RoundingMode::Down {
                round_increment = 0;
            }
        }
    }
    let mut round_bits = z_sig & 0x7F;
    if z_exp as u16 >= 0xFD {
        if z_exp > 0xFD || (z_exp == 0xFD && (z_sig.wrapping_add(round_increment) as i32) < 0) {
            float_raise(flag::OVERFLOW | flag::INEXACT);
            return pack_float32(z_sign, 0xFF, 0).wrapping_sub(u32::from(round_increment == 0));
        }
        if z_exp < 0 {
            let is_tiny = z_exp < -1 || z_sig.wrapping_add(round_increment) < 0x8000_0000;
            z_sig = shift32_right_jamming(z_sig, -z_exp);
            z_exp = 0;
            round_bits = z_sig & 0x7F;
            if is_tiny && round_bits != 0 {
                float_raise(flag::UNDERFLOW);
            }
        }
    }
    if round_bits != 0 {
        float_raise(flag::INEXACT);
    }
    z_sig = z_sig.wrapping_add(round_increment) >> 7;
    z_sig &= !u32::from((round_bits ^ 0x40) == 0 && round_nearest_even);
    if z_sig == 0 {
        z_exp = 0;
    }
    pack_float32(z_sign, z_exp, z_sig)
}

/// `roundAndPackFloat64`: round a 64-bit significand (10 guard bits) to
/// double precision under `mode`. Value-only.
#[must_use]
fn round_and_pack_float64(mode: RoundingMode, z_sign: bool, mut z_exp: i32, mut z_sig: u64) -> u64 {
    let round_nearest_even = mode == RoundingMode::NearestEven;
    let mut round_increment: u64 = 0x200;
    if !round_nearest_even {
        if mode == RoundingMode::Zero {
            round_increment = 0;
        } else {
            round_increment = 0x3FF;
            if z_sign {
                if mode == RoundingMode::Up {
                    round_increment = 0;
                }
            } else if mode == RoundingMode::Down {
                round_increment = 0;
            }
        }
    }
    let mut round_bits = z_sig & 0x3FF;
    if z_exp as u16 >= 0x7FD {
        if z_exp > 0x7FD || (z_exp == 0x7FD && (z_sig.wrapping_add(round_increment) as i64) < 0) {
            float_raise(flag::OVERFLOW | flag::INEXACT);
            return pack_float64(z_sign, 0x7FF, 0).wrapping_sub(u64::from(round_increment == 0));
        }
        if z_exp < 0 {
            let is_tiny = z_exp < -1 || z_sig.wrapping_add(round_increment) < 0x8000_0000_0000_0000;
            z_sig = shift64_right_jamming(z_sig, -z_exp);
            z_exp = 0;
            round_bits = z_sig & 0x3FF;
            if is_tiny && round_bits != 0 {
                float_raise(flag::UNDERFLOW);
            }
        }
    }
    if round_bits != 0 {
        float_raise(flag::INEXACT);
    }
    z_sig = z_sig.wrapping_add(round_increment) >> 10;
    z_sig &= !u64::from((round_bits ^ 0x200) == 0 && round_nearest_even);
    if z_sig == 0 {
        z_exp = 0;
    }
    pack_float64(z_sign, z_exp, z_sig)
}

/// `floatx80_to_float32`: narrow an extended value to an IEEE single
/// (raw 32-bit bit pattern), rounding per `mode`.
#[must_use]
pub fn floatx80_to_float32(mode: RoundingMode, a: FpReg) -> u32 {
    let mut a_sig = frac(a);
    let mut a_exp = exp(a);
    let a_sign = sign(a);
    if a_exp == 0x7FFF {
        if a_sig.wrapping_shl(1) != 0 {
            return floatx80_nan_to_float32(a);
        }
        return pack_float32(a_sign, 0xFF, 0);
    }
    a_sig = shift64_right_jamming(a_sig, 33);
    if a_exp != 0 || a_sig != 0 {
        a_exp -= 0x3F81;
    }
    round_and_pack_float32(mode, a_sign, a_exp, a_sig as u32)
}

/// `floatx80_to_float64`: narrow an extended value to an IEEE double
/// (raw 64-bit bit pattern), rounding per `mode`.
#[must_use]
pub fn floatx80_to_float64(mode: RoundingMode, a: FpReg) -> u64 {
    let a_sig = frac(a);
    let mut a_exp = exp(a);
    let a_sign = sign(a);
    if a_exp == 0x7FFF {
        if a_sig.wrapping_shl(1) != 0 {
            return floatx80_nan_to_float64(a);
        }
        return pack_float64(a_sign, 0x7FF, 0);
    }
    let z_sig = shift64_right_jamming(a_sig, 1);
    if a_exp != 0 || a_sig != 0 {
        a_exp -= 0x3C01;
    }
    round_and_pack_float64(mode, a_sign, a_exp, z_sig)
}

// --- FMOVECR constant ROM (m68kfpu.c) ---

/// IEEE-double bit pattern of `1e256`, the seed for the high powers of ten.
const TEN_TO_256: u64 = 0x7515_4FDD_7F73_BF3C;

/// `FMOVECR #ccc`: the 68881/2 on-chip constant ROM. Returns the
/// extended-precision constant at ROM offset `offset` (the low 7 bits of
/// the FP extension word). Built exactly as Musashi's `fpgen_rm_reg`
/// FMOVECR case: literal `floatx80` patterns for the named constants,
/// `int32_to_floatx80` for the small integer powers, `float64_to_floatx80`
/// of the `1eN` double bit patterns for 10^8…10^256, and repeated
/// `floatx80_mul` squaring (under the given rounding mode) for the higher
/// powers. Unlisted offsets read as +0.0, matching the oracle's default.
#[must_use]
pub fn fmovecr(mode: RoundingMode, offset: u8) -> FpReg {
    let ten256 = || float64_to_floatx80(TEN_TO_256);
    let sq = |v: FpReg| floatx80_mul(80, mode, v, v);
    match offset {
        0x00 => FpReg::new(0x4000, 0xC90F_DAA2_2168_C235), // pi
        0x0B => FpReg::new(0x3FFD, 0x9A20_9A84_FBCF_F798), // log10(2)
        0x0C => FpReg::new(0x4000, 0xADF8_5458_A2BB_4A9B), // e
        0x0D => FpReg::new(0x3FFF, 0xB8AA_3B29_5C17_F0BC), // log2(e)
        0x0E => FpReg::new(0x3FFD, 0xDE5B_D8A9_3728_7195), // log10(e)
        0x0F => int32_to_floatx80(0),                      // 0.0
        0x30 => FpReg::new(0x3FFE, 0xB172_17F7_D1CF_79AC), // ln(2)
        0x31 => FpReg::new(0x4000, 0x935D_8DDD_AAA8_AC17), // ln(10)
        0x32 => int32_to_floatx80(1),                      // 1
        0x33 => int32_to_floatx80(10),                     // 10^1
        0x34 => int32_to_floatx80(100),                    // 10^2
        0x35 => int32_to_floatx80(10000),                  // 10^4
        0x36 => float64_to_floatx80(0x4197_D784_0000_0000), // 10^8
        0x37 => float64_to_floatx80(0x4341_C379_37E0_8000), // 10^16
        0x38 => float64_to_floatx80(0x4693_B8B5_B505_6E17), // 10^32
        0x39 => float64_to_floatx80(0x4D38_4F03_E93F_F9F5), // 10^64
        0x3A => float64_to_floatx80(0x5A82_7748_F930_1D32), // 10^128
        0x3B => float64_to_floatx80(TEN_TO_256),           // 10^256
        0x3C => sq(ten256()),                              // 10^512
        0x3D => sq(sq(ten256())),                          // 10^1024
        0x3E => sq(sq(sq(ten256()))),                      // 10^2048
        0x3F => sq(sq(sq(sq(ten256())))),                  // 10^4096
        _ => int32_to_floatx80(0),                         // default → +0.0
    }
}

// --- Packed-decimal real conversion (softfloat_decimal.cpp) ---
//
// A faithful port of WinUAE's `softfloat_decimal.cpp` (Andreas Grabher /
// Previous lineage), the silicon-validated 68881/2 packed-decimal converter.
// These routines move between a `floatx80` and an *intermediate decimal*
// representation — a `floatx80` whose 64-bit significand holds the decimal
// significand as a plain integer (up to 17 digits) and whose 15-bit exponent
// field holds the magnitude of the decimal power-of-ten (bit 14 = its sign).
// The 96-bit BCD memory format is shuffled to/from that intermediate by
// [`pack_decimal_to_floatx80`] / [`floatx80_to_pack_decimal`] below, mirroring
// WinUAE's `fp_to_pack` / `fp_from_pack`.

/// `mul128By64To192`: 128×64→192 product of (`a0`,`a1`)·`b`.
#[must_use]
fn mul128_by64_to_192(a0: u64, a1: u64, b: u64) -> (u64, u64, u64) {
    let (z1, z2) = mul64_to_128(a1, b);
    let (z0, more1) = mul64_to_128(a0, b);
    let (z0, z1) = add128(z0, more1, 0, z1);
    (z0, z1, z2)
}

/// `mul128To256`: 128×128→256 product of (`a0`,`a1`)·(`b0`,`b1`).
#[must_use]
fn mul128_to_256(a0: u64, a1: u64, b0: u64, b1: u64) -> (u64, u64, u64, u64) {
    let (z2, z3) = mul64_to_128(a1, b1);
    let (z1, more2) = mul64_to_128(a1, b0);
    let (z1, z2) = add128(z1, more2, 0, z2);
    let (z0, more1) = mul64_to_128(a0, b0);
    let (z0, z1) = add128(z0, more1, 0, z1);
    let (more1, more2) = mul64_to_128(a0, b1);
    let (more1, z2) = add128(more1, more2, 0, z2);
    let (z0, z1) = add128(z0, z1, 0, more1);
    (z0, z1, z2, z3)
}

/// `round128to64`: round the 128-bit significand (`a_sig0`,`a_sig1`) to 64
/// bits under `mode`, adjusting the exponent. Returns `(exp, sig0, 0)`.
#[must_use]
fn round128to64(
    a_sign: bool,
    a_exp: i32,
    a_sig0: u64,
    a_sig1: u64,
    mode: RoundingMode,
) -> (i32, u64, u64) {
    let mut z_exp = a_exp;
    let mut z_sig0 = a_sig0;
    let z_sig1 = a_sig1;
    let mut increment = (z_sig1 as i64) < 0;
    match mode {
        RoundingMode::NearestEven => {}
        RoundingMode::Zero => increment = false,
        RoundingMode::Down => increment = a_sign && z_sig1 != 0,
        RoundingMode::Up => increment = !a_sign && z_sig1 != 0,
    }
    if increment {
        z_sig0 = z_sig0.wrapping_add(1);
        if z_sig0 == 0 {
            z_exp += 1;
            z_sig0 = 0x8000_0000_0000_0000;
        } else if (z_sig1 << 1) == 0 && mode == RoundingMode::NearestEven {
            z_sig0 &= !1;
        }
    } else if z_sig0 == 0 {
        z_exp = 0;
    }
    (z_exp, z_sig0, 0)
}

/// `mul128by128round`: multiply (`a`) by (`b`) — both 128-bit fixed-point
/// significands with biased exponents — rounding the product to 64 bits.
#[must_use]
fn mul128by128round(
    a_exp: i32,
    a_sig0: u64,
    a_sig1: u64,
    b_exp: i32,
    b_sig0: u64,
    b_sig1: u64,
    mode: RoundingMode,
) -> (i32, u64, u64) {
    let (b_exp, b_sig0, b_sig1) = round128to64(false, b_exp, b_sig0, b_sig1, mode);
    let mut z_exp = a_exp + b_exp - 0x3FFE;
    let (mut z_sig0, mut z_sig1, z_sig2, z_sig3) = mul128_to_256(a_sig0, a_sig1, b_sig0, b_sig1);
    z_sig1 |= u64::from((z_sig2 | z_sig3) != 0);
    if (z_sig0 as i64) > 0 {
        let (s0, s1) = short_shift128_left(z_sig0, z_sig1, 1);
        z_sig0 = s0;
        z_sig1 = s1;
        z_exp -= 1;
    }
    round128to64(false, z_exp, z_sig0, z_sig1, mode)
}

/// `mul128by128`: multiply two 128-bit fixed-point significands (no rounding
/// to 64 bits — the full 128-bit product significand is kept).
#[must_use]
fn mul128by128(
    a_exp: i32,
    a_sig0: u64,
    a_sig1: u64,
    b_exp: i32,
    b_sig0: u64,
    b_sig1: u64,
) -> (i32, u64, u64) {
    let mut z_exp = a_exp + b_exp - 0x3FFE;
    let (mut z_sig0, mut z_sig1, z_sig2, z_sig3) = mul128_to_256(a_sig0, a_sig1, b_sig0, b_sig1);
    z_sig1 |= u64::from((z_sig2 | z_sig3) != 0);
    if (z_sig0 as i64) > 0 {
        let (s0, s1) = short_shift128_left(z_sig0, z_sig1, 1);
        z_sig0 = s0;
        z_sig1 = s1;
        z_exp -= 1;
    }
    (z_exp, z_sig0, z_sig1)
}

/// `div128by128`: divide (`a`) by (`b`), both 128-bit fixed-point
/// significands with biased exponents.
#[must_use]
fn div128by128(
    a_exp: i32,
    a_sig0: u64,
    a_sig1: u64,
    b_exp: i32,
    b_sig0: u64,
    b_sig1: u64,
) -> (i32, u64, u64) {
    let mut z_exp = a_exp - b_exp + 0x3FFE;
    let (mut a_sig0, mut a_sig1) = (a_sig0, a_sig1);
    if le128(b_sig0, b_sig1, a_sig0, a_sig1) {
        let (s0, s1) = shift128_right(a_sig0, a_sig1, 1);
        a_sig0 = s0;
        a_sig1 = s1;
        z_exp += 1;
    }
    let mut z_sig0 = estimate_div128_to_64(a_sig0, a_sig1, b_sig0);
    let (term0, term1, term2) = mul128_by64_to_192(b_sig0, b_sig1, z_sig0);
    let (mut rem0, mut rem1, mut rem2) = sub192(a_sig0, a_sig1, 0, term0, term1, term2);
    while (rem0 as i64) < 0 {
        z_sig0 = z_sig0.wrapping_sub(1);
        let (r0, r1, r2) = add192(rem0, rem1, rem2, 0, b_sig0, b_sig1);
        rem0 = r0;
        rem1 = r1;
        rem2 = r2;
    }
    let mut z_sig1 = estimate_div128_to_64(rem1, rem2, b_sig0);
    if (z_sig1 & 0x3FFF) <= 4 {
        let (term1b, term2b, term3b) = mul128_by64_to_192(b_sig0, b_sig1, z_sig1);
        let (mut rem1b, mut rem2b, mut rem3b) = sub192(rem1, rem2, 0, term1b, term2b, term3b);
        while (rem1b as i64) < 0 {
            z_sig1 = z_sig1.wrapping_sub(1);
            let (r1, r2, r3) = add192(rem1b, rem2b, rem3b, 0, b_sig0, b_sig1);
            rem1b = r1;
            rem2b = r2;
            rem3b = r3;
        }
        z_sig1 |= u64::from((rem1b | rem2b | rem3b) != 0);
    }
    (z_exp, z_sig0, z_sig1)
}

/// `tentoint128`: compute 10^`scale` as a 128-bit fixed-point significand.
/// `m_sign`/`e_sign` (the significand and exponent signs of the value being
/// scaled) bias the intermediate rounding mode exactly as the 68881/2 does so
/// the round-trip is direction-stable.
#[must_use]
fn tentoint128(m_sign: bool, e_sign: bool, scale: i32, mode: RoundingMode) -> (i32, u64, u64) {
    // The 68881/2 temporarily skews the rounding mode while building 10^scale
    // so that decimal→binary and binary→decimal stay mutually consistent.
    let inner = match mode {
        RoundingMode::NearestEven => RoundingMode::NearestEven,
        RoundingMode::Down => {
            if m_sign != e_sign {
                RoundingMode::Up
            } else {
                RoundingMode::Down
            }
        }
        RoundingMode::Up => {
            if m_sign != e_sign {
                RoundingMode::Down
            } else {
                RoundingMode::Up
            }
        }
        RoundingMode::Zero => {
            if !e_sign {
                RoundingMode::Down
            } else {
                RoundingMode::Up
            }
        }
    };

    let mut a_exp = 0x3FFF;
    let mut a_sig0 = 0x8000_0000_0000_0000u64;
    let mut a_sig1 = 0u64;
    let mut m_exp = 0x4002;
    let mut m_sig0 = 0xA000_0000_0000_0000u64;
    let mut m_sig1 = 0u64;
    let mut scale = scale;
    while scale != 0 {
        if scale & 1 != 0 {
            let (e, s0, s1) = mul128by128round(a_exp, a_sig0, a_sig1, m_exp, m_sig0, m_sig1, inner);
            a_exp = e;
            a_sig0 = s0;
            a_sig1 = s1;
        }
        let (e, s0, s1) = mul128by128(m_exp, m_sig0, m_sig1, m_exp, m_sig0, m_sig1);
        m_exp = e;
        m_sig0 = s0;
        m_sig1 = s1;
        scale >>= 1;
    }
    (a_exp, a_sig0, a_sig1)
}

/// `tentointdec`: compute 10^`scale` as a 64-bit decimal integer (wraps past
/// 2^64 exactly as the C `uint64_t` arithmetic does — only the in-range
/// result is consumed).
#[must_use]
fn tentointdec(scale: i32) -> i64 {
    let mut dec_x: u64 = 1;
    let mut dec_m: u64 = 10;
    let mut scale = scale;
    while scale != 0 {
        if scale & 1 != 0 {
            dec_x = dec_x.wrapping_mul(dec_m);
        }
        dec_m = dec_m.wrapping_mul(dec_m);
        scale >>= 1;
    }
    dec_x as i64
}

/// `float128toint64`: round the 128-bit value to a 64-bit integer under
/// `mode`, raising `inexact` if anything was discarded.
#[must_use]
fn float128toint64(z_sign: bool, z_exp: i32, z_sig0: u64, z_sig1: u64, mode: RoundingMode) -> i64 {
    let (mut z_sig0, z_sig1) = shift128_right_jamming(z_sig0, z_sig1, 0x403E - z_exp);
    let round_nearest_even = mode == RoundingMode::NearestEven;
    let mut increment = (z_sig1 as i64) < 0;
    if !round_nearest_even {
        match mode {
            RoundingMode::Zero => increment = false,
            RoundingMode::Down => increment = z_sign && z_sig1 != 0,
            RoundingMode::Up => increment = !z_sign && z_sig1 != 0,
            RoundingMode::NearestEven => {}
        }
    }
    if increment {
        z_sig0 = z_sig0.wrapping_add(1);
        if (z_sig1 << 1) == 0 && round_nearest_even {
            z_sig0 &= !1;
        }
    }
    if z_sig1 != 0 {
        float_raise(flag::INEXACT);
    }
    z_sig0 as i64
}

/// `getDecimalExponent`: estimate the base-10 exponent of a `floatx80`, used
/// to seed the binary→decimal digit count.
#[must_use]
fn get_decimal_exponent(a_exp: i32, a_sig: u64) -> i32 {
    if a_sig == 0 || a_exp == 0x3FFF {
        return 0;
    }
    if a_exp < 0 {
        return -4932;
    }
    let mut a_sig = a_sig ^ 0x8000_0000_0000_0000;
    let a_exp_unb = a_exp - 0x3FFF;
    let z_sign = a_exp_unb < 0;
    let a_exp_abs = if z_sign { -a_exp_unb } else { a_exp_unb };
    let mut shift_count = 31 - (a_exp_abs as u32).leading_zeros() as i32;
    let mut z_exp = 0x3FFF + shift_count;
    let (mut z_sig0, mut z_sig1);
    if shift_count < 0 {
        let (s0, s1) = short_shift128_left(a_sig, 0, -shift_count);
        z_sig0 = s0;
        z_sig1 = s1;
    } else {
        let (s0, s1) = shift128_right(a_sig, 0, shift_count);
        z_sig0 = s0;
        z_sig1 = s1;
        a_sig = (a_exp_abs as u64) << (63 - shift_count);
        let (r0, r1) = if z_sign {
            sub128(a_sig, 0, z_sig0, z_sig1)
        } else {
            add128(a_sig, 0, z_sig0, z_sig1)
        };
        z_sig0 = r0;
        z_sig1 = r1;
    }
    shift_count = z_sig0.leading_zeros() as i32;
    let (s0, s1) = short_shift128_left(z_sig0, z_sig1, shift_count);
    z_sig0 = s0;
    z_sig1 = s1;
    z_exp -= shift_count;
    // × log10(2) in 128-bit fixed point.
    let (e, s0, s1) = mul128by128(
        z_exp,
        z_sig0,
        z_sig1,
        0x3FFD,
        0x9A20_9A84_FBCF_F798,
        0x8F89_59AC_0B7C_9178,
    );
    z_exp = e;
    z_sig0 = s0;
    z_sig1 = s1;
    shift_count = 0x403E - z_exp;
    let (s0, s1) = shift128_right_jamming(z_sig0, z_sig1, shift_count);
    z_sig0 = s0;
    z_sig1 = s1;
    if (z_sig1 as i64) < 0 {
        z_sig0 = z_sig0.wrapping_add(1);
        if (z_sig1 << 1) == 0 {
            z_sig0 &= !1;
        }
    }
    if z_sign {
        0u64.wrapping_sub(z_sig0) as i32
    } else {
        z_sig0 as i32
    }
}

/// `floatdecimal_to_floatx80`: convert the intermediate decimal representation
/// (significand digits in `frac`, decimal power-of-ten in the exponent field
/// with bit 14 its sign) to a binary `floatx80`.
#[must_use]
pub fn floatdecimal_to_floatx80(a: FpReg, mode: RoundingMode) -> FpReg {
    let dec_sign = sign(a);
    let dec_exp_raw = exp(a);
    let dec_sig = frac(a);

    if dec_exp_raw == 0x7FFF {
        return a;
    }
    if dec_exp_raw == 0 && dec_sig == 0 {
        return a;
    }

    let dec_exp_sign = (dec_exp_raw >> 14) & 1 != 0;
    let dec_exp = dec_exp_raw & 0x3FFF;

    let shift_count = dec_sig.leading_zeros() as i32;
    let z_exp = 0x403E - shift_count;
    let z_sig0 = dec_sig.wrapping_shl(shift_count as u32);
    let z_sig1 = 0u64;
    let z_sign = dec_sign;

    let (x_exp, x_sig0, x_sig1) = tentoint128(dec_sign, dec_exp_sign, dec_exp, mode);

    let (z_exp, z_sig0, z_sig1) = if dec_exp_sign {
        div128by128(z_exp, z_sig0, z_sig1, x_exp, x_sig0, x_sig1)
    } else {
        mul128by128(z_exp, z_sig0, z_sig1, x_exp, x_sig0, x_sig1)
    };

    if z_sig1 != 0 {
        float_raise(flag::DECIMAL);
    }
    let (z_exp, z_sig0, _) = round128to64(z_sign, z_exp, z_sig0, z_sig1, mode);

    pack(z_sign, z_exp, z_sig0)
}

/// `floatx80_to_floatdecimal`: convert a binary `floatx80` to the intermediate
/// decimal representation. `k` is the k-factor on entry (a signed digit count,
/// −64..17) and receives the actual significand-digit count (`len`, 1..17) on
/// return — exactly the in/out contract WinUAE's `fp_from_pack` relies on.
#[must_use]
pub fn floatx80_to_floatdecimal(a: FpReg, k: &mut i32, mode: RoundingMode) -> FpReg {
    let a_sign = sign(a);
    let mut a_exp = exp(a);
    let mut a_sig = frac(a);

    if a_exp == 0x7FFF {
        if (a_sig << 1) != 0 {
            return propagate_floatx80_nan_one_arg(a);
        }
        return a;
    }
    if a_exp == 0 {
        if a_sig == 0 {
            return pack(a_sign, 0, 0);
        }
        let (e, s) = normalize_floatx80_subnormal(a_sig);
        a_exp = e;
        a_sig = s;
    }

    let mut kfactor = *k;
    let mut ilog = get_decimal_exponent(a_exp, a_sig);
    let mut ictr = false;
    let mut len;
    let dec_sig;

    loop {
        if kfactor > 0 {
            if kfactor > 17 {
                kfactor = 17;
                float_raise(flag::INVALID);
            }
            len = kfactor;
        } else {
            // C: `if (len>17) len=17; if (len<1) len=1;` — order-equivalent to
            // clamp(1, 17) since 17 ≥ 1.
            len = (ilog + 1 - kfactor).clamp(1, 17);
            if kfactor > ilog {
                ilog = kfactor;
            }
        }

        let mut lambda = false;
        let mut iscale = ilog + 1 - len;
        if iscale < 0 {
            lambda = true;
            iscale = -iscale;
        }

        let (x_exp, x_sig0, x_sig1) = tentoint128(lambda, false, iscale, mode);

        let z_exp = a_exp;
        let z_sig0 = a_sig;
        let z_sig1 = 0u64;
        let (z_exp, z_sig0, z_sig1) = if lambda {
            mul128by128(z_exp, z_sig0, z_sig1, x_exp, x_sig0, x_sig1)
        } else {
            div128by128(z_exp, z_sig0, z_sig1, x_exp, x_sig0, x_sig1)
        };

        let candidate = float128toint64(a_sign, z_exp, z_sig0, z_sig1, mode);

        if !ictr {
            let dec_x = tentointdec(len - 1);
            if candidate < dec_x {
                ilog -= 1;
                ictr = true;
                continue;
            }
            if candidate > dec_x * 10 {
                ilog += 1;
                ictr = true;
                continue;
            }
        }
        dec_sig = candidate;
        break;
    }

    let dec_sign = a_sign;
    let mut dec_exp = if ilog < 0 { -ilog } else { ilog };
    if dec_exp > 999 {
        float_raise(flag::INVALID);
    }
    if ilog < 0 {
        dec_exp |= 0x4000;
    }

    *k = len;
    pack(dec_sign, dec_exp, dec_sig as u64)
}

/// Convert the 96-bit packed-decimal memory operand (three big-endian
/// longwords `wrd[0..3]`, M68000PRM § 1.6) to a binary `floatx80`. Mirrors
/// WinUAE's `fp_to_pack`: ±∞ / NaN / ±0 copy through the extended layout, and
/// finite values are decoded into the intermediate decimal form and run
/// through [`floatdecimal_to_floatx80`].
#[must_use]
pub fn pack_decimal_to_floatx80(wrd: [u32; 3], mode: RoundingMode) -> FpReg {
    // Infinity (extended exponent, zero packed fraction) and NaNs copy bit for
    // bit through the extended layout (high word, then the 64-bit mantissa).
    if (wrd[0] >> 16) & 0x7FFF == 0x7FFF {
        return FpReg::new(
            (wrd[0] >> 16) as u16,
            (u64::from(wrd[1]) << 32) | u64::from(wrd[2]),
        );
    }
    // Zero significand: the exponent is irrelevant, keep only the sign.
    if wrd[0] & 0xF == 0 && wrd[1] == 0 && wrd[2] == 0 {
        return FpReg::new((wrd[0] >> 16) as u16 & 0x8000, 0);
    }

    let pack_exp = (wrd[0] >> 16) & 0xFFF; // three exponent digits
    let pack_int = wrd[0] & 0xF; // integer digit
    let pack_frac = (u64::from(wrd[1]) << 32) | u64::from(wrd[2]); // 16 fraction digits
    let pack_se = (wrd[0] >> 30) & 1 != 0; // exponent sign
    let pack_sm = (wrd[0] >> 31) & 1 != 0; // significand sign

    let mut dec_exp: i32 = 0;
    for i in 0..3 {
        dec_exp *= 10;
        dec_exp += ((pack_exp >> (8 - i * 4)) & 0xF) as i32;
    }
    if pack_se {
        dec_exp = -dec_exp;
    }
    // The 17-digit significand is treated as an integer; shift the decimal
    // point past the 16 fraction digits.
    dec_exp -= 16;
    let mut neg_exp = false;
    if dec_exp < 0 {
        dec_exp = -dec_exp;
        neg_exp = true;
    }

    let mut mant: i64 = i64::from(pack_int);
    for i in 0..16 {
        mant *= 10;
        mant += ((pack_frac >> (60 - i * 4)) & 0xF) as i64;
    }

    let high = (dec_exp as u16 & 0x3FFF)
        | if neg_exp { 0x4000 } else { 0 }
        | if pack_sm { 0x8000 } else { 0 };
    floatdecimal_to_floatx80(FpReg::new(high, mant as u64), mode)
}

/// Convert a binary `floatx80` to the 96-bit packed-decimal memory operand
/// under the requested `kfactor` (−64..17). Mirrors WinUAE's `fp_from_pack`:
/// ∞ / NaN copy through the extended layout, finite values are formatted from
/// the [`floatx80_to_floatdecimal`] intermediate into BCD digits.
#[must_use]
pub fn floatx80_to_pack_decimal(a: FpReg, kfactor: i32, mode: RoundingMode) -> [u32; 3] {
    let mut kfactor = kfactor;
    let f = floatx80_to_floatdecimal(a, &mut kfactor, mode);

    if f.high & 0x7FFF == 0x7FFF {
        // ∞ / NaN: bit-for-bit through the extended layout.
        return [u32::from(f.high) << 16, (f.low >> 32) as u32, f.low as u32];
    }

    let exponent = u32::from(f.high) & 0x3FFF;
    let significand = f.low;

    // Extract `len` significand digits (LSB first): the last becomes the
    // integer digit, the rest the fraction nibbles.
    let mut pack_int: u32 = 0;
    let mut pack_frac: u64 = 0;
    let mut len = kfactor; // SoftFloat returned the digit count in kfactor.
    let mut sig = significand;
    while len > 0 {
        len -= 1;
        let digit = sig % 10;
        sig /= 10;
        if len == 0 {
            pack_int = digit as u32;
        } else {
            pack_frac |= digit << (64 - len * 4);
        }
    }

    // Extract the four exponent digits (LSB first); the fourth is the
    // thousands digit (only non-zero for the ±inf/NaN encodings handled above).
    let mut pack_exp: u32 = 0;
    let mut pack_exp4: u32 = 0;
    let mut len = 4;
    let mut e = exponent;
    while len > 0 {
        len -= 1;
        let digit = e % 10;
        e /= 10;
        if len == 0 {
            pack_exp4 = digit;
        } else {
            pack_exp |= digit << (12 - len * 4);
        }
    }

    let pack_se = f.high & 0x4000 != 0;
    let pack_sm = f.high & 0x8000 != 0;

    let wrd0 = (pack_exp << 16)
        | (pack_exp4 << 12)
        | pack_int
        | if pack_se { 0x4000_0000 } else { 0 }
        | if pack_sm { 0x8000_0000 } else { 0 };
    [wrd0, (pack_frac >> 32) as u32, pack_frac as u32]
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn int32_zero_is_positive_zero() {
        assert_eq!(int32_to_floatx80(0), FpReg::new(0, 0));
    }

    #[test]
    fn pseudo_encoding_classification() {
        // Unnormal: normal exponent, integer bit clear, non-zero.
        assert!(floatx80_is_unnormal(FpReg::new(
            0x4001,
            0x4000_0000_0000_0000
        )));
        // Normal (integer bit set) is not unnormal.
        assert!(!floatx80_is_unnormal(FpReg::new(
            0x4001,
            0x8000_0000_0000_0000
        )));
        // Denormal: zero exponent, integer bit clear, non-zero fraction.
        assert!(floatx80_is_denormal(FpReg::new(0, 0x4000_0000_0000_0000)));
        // Pseudo-denormal (exp 0, integer bit set) is neither unnormal nor denormal.
        let pseudo_denorm = FpReg::new(0, 0x8000_0000_0000_0000);
        assert!(!floatx80_is_unnormal(pseudo_denorm));
        assert!(!floatx80_is_denormal(pseudo_denorm));
        // Zero is neither.
        assert!(!floatx80_is_unnormal(FpReg::new(0, 0)));
        assert!(!floatx80_is_denormal(FpReg::new(0, 0)));
    }

    #[test]
    fn normalize_shifts_unnormal_significand() {
        // 0x4001:0x4000…0 (= 2.0, unnormal) normalises to 0x4000:0x8000…0.
        assert_eq!(
            floatx80_normalize(FpReg::new(0x4001, 0x4000_0000_0000_0000)),
            FpReg::new(0x4000, 0x8000_0000_0000_0000)
        );
        // A normal value is unchanged.
        let normal = FpReg::new(0x4000, 0x8000_0000_0000_0000);
        assert_eq!(floatx80_normalize(normal), normal);
        // An unnormal-zero (normal exponent, zero significand) collapses to ±0.
        assert_eq!(floatx80_normalize(FpReg::new(0x4001, 0)), FpReg::new(0, 0));
        assert_eq!(
            floatx80_normalize(FpReg::new(0xC001, 0)),
            FpReg::new(0x8000, 0)
        );
        // Infinities / zero-exponent values pass through (the integer-bit shift
        // would be wrong for denormals — the 68881/2 leaves them be).
        let inf = FpReg::new(0x7FFF, 0);
        assert_eq!(floatx80_normalize(inf), inf);
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
        // 68k convention: exponent is `-shiftCount`.
        let (exp, sig) = normalize_floatx80_subnormal(0x4000_0000_0000_0000);
        assert_eq!(exp, -1);
        assert_eq!(sig, 0x8000_0000_0000_0000);

        let (exp, sig) = normalize_floatx80_subnormal(1);
        assert_eq!(exp, -63);
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
            FpReg::new(0x7FFF, 0xFFFF_FFFF_FFFF_FFFF)
        );
    }

    #[test]
    fn rounding_mode_decodes_fpcr_bits() {
        assert_eq!(RoundingMode::from_fpcr_bits(0), RoundingMode::NearestEven);
        assert_eq!(RoundingMode::from_fpcr_bits(1), RoundingMode::Zero);
        assert_eq!(RoundingMode::from_fpcr_bits(2), RoundingMode::Down);
        assert_eq!(RoundingMode::from_fpcr_bits(3), RoundingMode::Up);
    }

    // --- Exact-value mul/div (powers of two and small integers stay exact). ---

    #[test]
    fn mul_two_times_three_is_six() {
        assert_eq!(floatx80_mul(P80, RN, fx(2), fx(3)), fx(6));
    }

    #[test]
    fn mul_is_commutative_on_exact_values() {
        assert_eq!(
            floatx80_mul(P80, RN, fx(7), fx(3)),
            floatx80_mul(P80, RN, fx(3), fx(7))
        );
        assert_eq!(floatx80_mul(P80, RN, fx(7), fx(3)), fx(21));
    }

    #[test]
    fn mul_sign_rules() {
        assert_eq!(floatx80_mul(P80, RN, fx(-2), fx(3)), fx(-6));
        assert_eq!(floatx80_mul(P80, RN, fx(-2), fx(-3)), fx(6));
    }

    #[test]
    fn mul_by_zero_is_zero() {
        assert_eq!(
            floatx80_mul(P80, RN, fx(5), FpReg::new(0, 0)),
            FpReg::new(0, 0)
        );
        // Negative × +0 → −0 (sign is the xor).
        assert_eq!(
            floatx80_mul(P80, RN, fx(-5), FpReg::new(0, 0)),
            FpReg::new(0x8000, 0)
        );
    }

    #[test]
    fn mul_one_is_identity() {
        assert_eq!(floatx80_mul(P80, RN, fx(42), fx(1)), fx(42));
    }

    #[test]
    fn div_ten_by_two_is_five() {
        assert_eq!(floatx80_div(P80, RN, fx(10), fx(2)), fx(5));
    }

    #[test]
    fn div_by_one_is_identity() {
        assert_eq!(floatx80_div(P80, RN, fx(42), fx(1)), fx(42));
    }

    #[test]
    fn div_sign_rules() {
        assert_eq!(floatx80_div(P80, RN, fx(-10), fx(2)), fx(-5));
        assert_eq!(floatx80_div(P80, RN, fx(-10), fx(-2)), fx(5));
    }

    #[test]
    fn div_self_is_one() {
        assert_eq!(floatx80_div(P80, RN, fx(7), fx(7)), fx(1));
    }

    #[test]
    fn div_by_zero_is_infinity() {
        // x / 0 = ±inf. The 68881/2 creates the infinity with a clear integer
        // bit ($7FFF:0), not the x87-style $7FFF:8000.
        assert_eq!(
            floatx80_div(P80, RN, fx(1), FpReg::new(0, 0)),
            FpReg::new(0x7FFF, 0)
        );
        assert_eq!(
            floatx80_div(P80, RN, fx(-1), FpReg::new(0, 0)),
            FpReg::new(0xFFFF, 0)
        );
    }

    #[test]
    fn div_zero_by_zero_is_default_nan() {
        assert_eq!(
            floatx80_div(P80, RN, FpReg::new(0, 0), FpReg::new(0, 0)),
            FpReg::new(0x7FFF, 0xFFFF_FFFF_FFFF_FFFF)
        );
    }

    #[test]
    fn div_three_halves_rounds_to_nearest() {
        // 3 / 2 = 1.5 exactly: 1.1b × 2^0 → exp 0x3FFF, sig 0xC000…
        assert_eq!(
            floatx80_div(P80, RN, fx(3), fx(2)),
            FpReg::new(0x3FFF, 0xC000_0000_0000_0000)
        );
    }

    #[test]
    fn mul_div_round_trip_on_inexact_value() {
        // (1 / 3) * 3 is NOT exactly 1 (1/3 is inexact), but the result is
        // deterministic and must match Musashi — covered by the C diff
        // harness. Here we just pin that division then multiply is stable.
        let third = floatx80_div(P80, RN, fx(1), fx(3));
        let back = floatx80_mul(P80, RN, third, fx(3));
        // 0.999… rounds back to exactly 1.0 under round-to-nearest.
        assert_eq!(back, fx(1));
    }

    // --- FSGLMUL / FSGLDIV (single-precision multiply / divide). ---

    #[test]
    fn sglmul_exact_small_values() {
        // Small integers fit in single precision, so the truncation and
        // single-precision rounding are no-ops: 2 × 3 = 6, sign rules hold.
        assert_eq!(floatx80_sglmul(RN, fx(2), fx(3)), fx(6));
        assert_eq!(floatx80_sglmul(RN, fx(-2), fx(3)), fx(-6));
        assert_eq!(
            floatx80_sglmul(RN, fx(5), FpReg::new(0, 0)),
            FpReg::new(0, 0)
        );
    }

    #[test]
    fn sgldiv_exact_small_values() {
        assert_eq!(floatx80_sgldiv(RN, fx(6), fx(2)), fx(3));
        assert_eq!(floatx80_sgldiv(RN, fx(-6), fx(2)), fx(-3));
    }

    #[test]
    fn move_abs_neg_at_extended_are_exact() {
        let x = FpReg::new(0x4002, 0xA53F_0000_0000_0001);
        assert_eq!(floatx80_move(P80, RN, x), x);
        assert_eq!(
            floatx80_abs(P80, RN, x),
            FpReg::new(0x4002, 0xA53F_0000_0000_0001)
        );
        assert_eq!(
            floatx80_neg(P80, RN, x),
            FpReg::new(0xC002, 0xA53F_0000_0000_0001)
        );
        // Sign rules on zero.
        assert_eq!(
            floatx80_abs(P80, RN, FpReg::new(0x8000, 0)),
            FpReg::new(0, 0)
        );
        assert_eq!(floatx80_neg(P80, RN, fx(0)), FpReg::new(0x8000, 0));
    }

    #[test]
    fn move_at_single_clears_low_mantissa_bits() {
        // A value with bits below single precision rounds (toward nearest);
        // the low 40 mantissa bits end up clear.
        let x = FpReg::new(0x3FFF, 0x8000_0000_0000_00FF);
        let r = floatx80_move(32, RN, x);
        assert_eq!(r.low & 0x0000_00FF_FFFF_FFFF, 0);
        assert_eq!(r, fx(1)); // the tail rounds away
    }

    #[test]
    fn rem_rounds_quotient_to_nearest() {
        // 5 rem 3: round(5/3) = 2 → 5 − 6 = −1, quotient 2.
        let r = floatx80_rem(P80, RN, fx(5), fx(3));
        assert_eq!(r.value, fx(-1));
        assert_eq!(r.quotient & 0x7F, 2);
        assert!(!r.sign);
    }

    #[test]
    fn mod_truncates_quotient() {
        // 5 mod 3: trunc(5/3) = 1 → 5 − 3 = 2, quotient 1.
        let r = floatx80_mod(P80, RN, fx(5), fx(3));
        assert_eq!(r.value, fx(2));
        assert_eq!(r.quotient & 0x7F, 1);
        assert!(!r.sign);
        // Sign of the quotient tracks operand signs.
        let r2 = floatx80_mod(P80, RN, fx(-5), fx(3));
        assert!(r2.sign);
    }

    #[test]
    fn sglmul_truncates_operands_to_single() {
        // An operand with bits below single precision is truncated before the
        // multiply, so FSGLMUL by 1.0 returns the single-rounded operand, not
        // the full extended value.
        let extra = FpReg::new(0x3FFF, 0x8000_0000_0000_00FF); // 1.0 + tiny tail
        let single = floatx80_sglmul(RN, extra, fx(1));
        assert_eq!(single, fx(1), "low bits below single precision are dropped");
    }

    // --- Square root (perfect squares stay exact; special cases match). ---

    #[test]
    fn sqrt_of_perfect_squares() {
        assert_eq!(floatx80_sqrt(P80, RN, fx(4)), fx(2));
        assert_eq!(floatx80_sqrt(P80, RN, fx(9)), fx(3));
        assert_eq!(floatx80_sqrt(P80, RN, fx(16)), fx(4));
        assert_eq!(floatx80_sqrt(P80, RN, fx(144)), fx(12));
    }

    #[test]
    fn sqrt_of_one_is_one() {
        assert_eq!(floatx80_sqrt(P80, RN, fx(1)), fx(1));
    }

    #[test]
    fn sqrt_of_zero_is_zero() {
        assert_eq!(floatx80_sqrt(P80, RN, FpReg::new(0, 0)), FpReg::new(0, 0));
        // sqrt(−0) = −0.
        assert_eq!(
            floatx80_sqrt(P80, RN, FpReg::new(0x8000, 0)),
            FpReg::new(0x8000, 0)
        );
    }

    #[test]
    fn sqrt_of_negative_is_default_nan() {
        assert_eq!(
            floatx80_sqrt(P80, RN, fx(-4)),
            FpReg::new(0x7FFF, 0xFFFF_FFFF_FFFF_FFFF)
        );
    }

    #[test]
    fn sqrt_of_positive_infinity_is_infinity() {
        let inf = FpReg::new(0x7FFF, 0x8000_0000_0000_0000);
        assert_eq!(floatx80_sqrt(P80, RN, inf), inf);
    }

    #[test]
    fn sqrt_of_two_is_known_irrational() {
        // √2 = 1.41421356… → the exact round-to-nearest extended value
        // (cross-checked against Musashi via the C diff harness).
        assert_eq!(
            floatx80_sqrt(P80, RN, fx(2)),
            FpReg::new(0x3FFF, 0xB504_F333_F9DE_6484)
        );
    }

    // --- FGETMAN / FGETEXP (FPSP exponent/mantissa). ---

    #[test]
    fn getman_extracts_mantissa_in_one_two_range() {
        // 6.0 = 1.5 × 2^2 → mantissa 1.5.
        assert_eq!(
            floatx80_getman(fx(6)),
            FpReg::new(0x3FFF, 0xC000_0000_0000_0000)
        );
        // 1.0 → mantissa 1.0.
        assert_eq!(
            floatx80_getman(fx(1)),
            FpReg::new(0x3FFF, 0x8000_0000_0000_0000)
        );
        // Sign of the source is preserved.
        assert_eq!(
            floatx80_getman(fx(-6)),
            FpReg::new(0xBFFF, 0xC000_0000_0000_0000)
        );
    }

    #[test]
    fn getman_of_zero_is_zero() {
        assert_eq!(floatx80_getman(fx(0)), FpReg::new(0, 0));
        assert_eq!(
            floatx80_getman(FpReg::new(0x8000, 0)),
            FpReg::new(0x8000, 0)
        );
    }

    #[test]
    fn getman_of_infinity_is_invalid_default_nan() {
        clear_exception_flags();
        let inf = FpReg::new(0x7FFF, 0x8000_0000_0000_0000);
        assert_eq!(floatx80_getman(inf), DEFAULT_NAN);
        assert_eq!(exception_flags() & flag::INVALID, flag::INVALID);
    }

    #[test]
    fn getexp_returns_unbiased_exponent_for_normals() {
        // 6.0 = 1.5 × 2^2 → exponent 2.0.
        assert_eq!(
            floatx80_getexp(fx(6)),
            FpReg::new(0x4000, 0x8000_0000_0000_0000)
        );
        // 1.0 → exponent 0.0.
        assert_eq!(floatx80_getexp(fx(1)), FpReg::new(0, 0));
        // 8.0 = 1.0 × 2^3 → exponent 3.0.
        assert_eq!(
            floatx80_getexp(fx(8)),
            FpReg::new(0x4000, 0xC000_0000_0000_0000)
        );
    }

    #[test]
    fn scale_multiplies_by_power_of_two() {
        // FSCALE truncates the second operand and adds it to the exponent.
        assert_eq!(floatx80_scale(P80, RN, fx(1), fx(2)), fx(4)); // 1 × 2^2
        assert_eq!(floatx80_scale(P80, RN, fx(6), fx(-1)), fx(3)); // 6 × 2^-1
        assert_eq!(floatx80_scale(P80, RN, fx(5), fx(0)), fx(5)); // unchanged
    }

    // --- Conversion to int32 (FINT / FINTRZ). ---

    #[test]
    fn to_int32_exact_integers() {
        assert_eq!(floatx80_to_int32(RN, fx(0)), 0);
        assert_eq!(floatx80_to_int32(RN, fx(42)), 42);
        assert_eq!(floatx80_to_int32(RN, fx(-42)), -42);
        assert_eq!(floatx80_to_int32(RN, fx(i32::MAX)), i32::MAX);
        assert_eq!(floatx80_to_int32(RN, fx(i32::MIN)), i32::MIN);
    }

    #[test]
    fn to_int32_rounds_to_nearest_even() {
        // 2.5 → 2 (ties to even), 3.5 → 4, 1.5 → 2.
        let two_point_five = FpReg::new(0x4000, 0xA000_0000_0000_0000);
        let three_point_five = FpReg::new(0x4000, 0xE000_0000_0000_0000);
        let one_point_five = FpReg::new(0x3FFF, 0xC000_0000_0000_0000);
        assert_eq!(floatx80_to_int32(RN, two_point_five), 2);
        assert_eq!(floatx80_to_int32(RN, three_point_five), 4);
        assert_eq!(floatx80_to_int32(RN, one_point_five), 2);
    }

    #[test]
    fn to_int32_round_to_zero_truncates() {
        let two_point_five = FpReg::new(0x4000, 0xA000_0000_0000_0000);
        let neg_two_point_five = FpReg::new(0xC000, 0xA000_0000_0000_0000);
        assert_eq!(floatx80_to_int32_round_to_zero(two_point_five), 2);
        assert_eq!(floatx80_to_int32_round_to_zero(neg_two_point_five), -2);
        // 0.9375 truncates to 0.
        assert_eq!(
            floatx80_to_int32_round_to_zero(FpReg::new(0x3FFE, 0xF000_0000_0000_0000)),
            0
        );
    }

    #[test]
    fn to_int32_round_modes_differ() {
        // 2.5: nearest-even → 2, toward-zero → 2, down → 2, up → 3.
        let v = FpReg::new(0x4000, 0xA000_0000_0000_0000);
        assert_eq!(floatx80_to_int32(RoundingMode::NearestEven, v), 2);
        assert_eq!(floatx80_to_int32(RoundingMode::Zero, v), 2);
        assert_eq!(floatx80_to_int32(RoundingMode::Down, v), 2);
        assert_eq!(floatx80_to_int32(RoundingMode::Up, v), 3);
    }

    #[test]
    fn to_int32_round_to_zero_saturates_on_overflow() {
        // 2^40 overflows int32 → saturate to MAX / MIN.
        let big = FpReg::new(0x4027, 0x8000_0000_0000_0000); // +2^40
        let neg_big = FpReg::new(0xC027, 0x8000_0000_0000_0000); // −2^40
        assert_eq!(floatx80_to_int32_round_to_zero(big), i32::MAX);
        assert_eq!(floatx80_to_int32_round_to_zero(neg_big), i32::MIN);
    }

    #[test]
    fn to_int32_of_nan_returns_mantissa_high_bits() {
        // 68k: floatx80_to_int32 of a NaN returns the high 32 bits of the
        // (quieted) mantissa, raising invalid — not a saturated value.
        let nan = FpReg::new(0x7FFF, 0xC000_0000_1234_5678);
        assert_eq!(
            flags_of(|| {
                assert_eq!(floatx80_to_int32(RN, nan), 0xC000_0000u32 as i32);
            }),
            flag::INVALID
        );
        // ±infinity still saturates.
        assert_eq!(floatx80_to_int32(RN, FpReg::new(0x7FFF, 0)), i32::MAX);
        assert_eq!(floatx80_to_int32(RN, FpReg::new(0xFFFF, 0)), i32::MIN);
    }

    // --- IEEE single / double → extended (exact widening). ---

    #[test]
    fn float32_widens_exactly() {
        // +1.0f = 0x3F800000 → 1.0 extended.
        assert_eq!(float32_to_floatx80(0x3F80_0000), fx(1));
        // −1.0f, +2.0f, +0.0f, −0.0f.
        assert_eq!(float32_to_floatx80(0xBF80_0000), fx(-1));
        assert_eq!(float32_to_floatx80(0x4000_0000), fx(2));
        assert_eq!(float32_to_floatx80(0x0000_0000), FpReg::new(0, 0));
        assert_eq!(float32_to_floatx80(0x8000_0000), FpReg::new(0x8000, 0));
    }

    #[test]
    fn float32_infinity_and_nan() {
        // 68k infinity: integer bit clear ($7FFF:0).
        assert_eq!(float32_to_floatx80(0x7F80_0000), FpReg::new(0x7FFF, 0));
        // A quiet NaN widens to a NaN (max exp, non-zero fraction).
        let nan = float32_to_floatx80(0x7FC0_0000);
        assert!(nan.is_nan());
    }

    #[test]
    fn float64_widens_exactly() {
        // +1.0 = 0x3FF0000000000000 → 1.0 extended.
        assert_eq!(float64_to_floatx80(0x3FF0_0000_0000_0000), fx(1));
        assert_eq!(float64_to_floatx80(0xBFF0_0000_0000_0000), fx(-1));
        assert_eq!(float64_to_floatx80(0x4000_0000_0000_0000), fx(2));
        assert_eq!(float64_to_floatx80(0), FpReg::new(0, 0));
        assert_eq!(
            float64_to_floatx80(0x8000_0000_0000_0000),
            FpReg::new(0x8000, 0)
        );
    }

    #[test]
    fn float64_infinity_and_nan() {
        // 68k infinity: integer bit clear ($7FFF:0).
        assert_eq!(
            float64_to_floatx80(0x7FF0_0000_0000_0000),
            FpReg::new(0x7FFF, 0)
        );
        assert!(float64_to_floatx80(0x7FF8_0000_0000_0000).is_nan());
    }

    #[test]
    fn float_widening_round_trips_through_arithmetic() {
        // 0.5f + 0.5f = 1.0 — exercises the widened operand in the adder.
        let half = float32_to_floatx80(0x3F00_0000); // 0.5f
        assert_eq!(floatx80_add(P80, RN, half, half), fx(1));
    }

    // --- extended → IEEE single / double (narrowing for memory stores). ---

    #[test]
    fn to_float32_exact_values() {
        assert_eq!(floatx80_to_float32(RN, fx(1)), 0x3F80_0000); // 1.0f
        assert_eq!(floatx80_to_float32(RN, fx(-1)), 0xBF80_0000);
        assert_eq!(floatx80_to_float32(RN, fx(2)), 0x4000_0000);
        assert_eq!(floatx80_to_float32(RN, FpReg::new(0, 0)), 0x0000_0000);
        assert_eq!(floatx80_to_float32(RN, FpReg::new(0x8000, 0)), 0x8000_0000);
    }

    #[test]
    fn to_float32_infinity_and_nan() {
        let inf = FpReg::new(0x7FFF, 0x8000_0000_0000_0000);
        assert_eq!(floatx80_to_float32(RN, inf), 0x7F80_0000);
        let nan = FpReg::new(0x7FFF, 0xC000_0000_0000_0000);
        assert_eq!(floatx80_to_float32(RN, nan) & 0x7F80_0000, 0x7F80_0000);
        assert_ne!(floatx80_to_float32(RN, nan) & 0x007F_FFFF, 0);
    }

    #[test]
    fn to_float64_exact_values() {
        assert_eq!(floatx80_to_float64(RN, fx(1)), 0x3FF0_0000_0000_0000);
        assert_eq!(floatx80_to_float64(RN, fx(-2)), 0xC000_0000_0000_0000);
        assert_eq!(floatx80_to_float64(RN, FpReg::new(0, 0)), 0);
    }

    #[test]
    fn narrowing_round_trips_for_representable_values() {
        // Values exactly representable in single/double widen-then-narrow
        // back to the same bit pattern.
        for bits in [0x3F80_0000_u32, 0xC080_0000, 0x4049_0FDB] {
            assert_eq!(floatx80_to_float32(RN, float32_to_floatx80(bits)), bits);
        }
        for bits in [0x3FF0_0000_0000_0000_u64, 0x4009_21FB_5444_2D18] {
            assert_eq!(floatx80_to_float64(RN, float64_to_floatx80(bits)), bits);
        }
    }

    #[test]
    fn to_float32_rounds_per_mode() {
        // π in extended → single. Round-to-nearest vs toward-zero differ in
        // the last bit (cross-checked against Musashi via the C harness).
        let pi = FpReg::new(0x4000, 0xC90F_DAA2_2168_C235);
        assert_eq!(
            floatx80_to_float32(RoundingMode::NearestEven, pi),
            0x4049_0FDB
        );
        assert_eq!(floatx80_to_float32(RoundingMode::Zero, pi), 0x4049_0FDA);
    }

    // --- Exception flags (the `float_raise` side, validated bit-for-bit
    // against softfloat.c's float_exception_flags by the C-diff harness;
    // see `examples/sf_gen.rs`). These pin the representative cases. ---

    /// Run `op` with the flag accumulator cleared, returning its flags.
    fn flags_of(op: impl FnOnce()) -> u8 {
        clear_exception_flags();
        op();
        take_exception_flags()
    }

    const INF: FpReg = FpReg::new(0x7FFF, 0x8000_0000_0000_0000);

    #[test]
    fn exact_arithmetic_raises_no_flags() {
        assert_eq!(
            flags_of(|| {
                let _ = floatx80_add(80, RN, fx(1), fx(1));
            }),
            0
        );
        assert_eq!(
            flags_of(|| {
                let _ = floatx80_mul(80, RN, fx(3), fx(4));
            }),
            0
        );
        assert_eq!(
            flags_of(|| {
                let _ = floatx80_sqrt(80, RN, fx(4));
            }),
            0
        );
    }

    #[test]
    fn inexact_division_raises_inexact() {
        assert_eq!(
            flags_of(|| {
                let _ = floatx80_div(80, RN, fx(1), fx(3));
            }),
            flag::INEXACT
        );
    }

    #[test]
    fn overflow_raises_overflow() {
        // 68k: OVERFLOW is raised unconditionally, but INEXACT only when
        // significand bits are actually discarded. huge + huge = exactly
        // 2×huge — the mantissa is exact, so only OVERFLOW is set.
        let huge = FpReg::new(0x7FFE, 0xFFFF_FFFF_FFFF_FFFF);
        assert_eq!(
            flags_of(|| {
                let _ = floatx80_add(80, RN, huge, huge);
            }),
            flag::OVERFLOW
        );
    }

    #[test]
    fn divide_by_zero_and_invalid_zero_over_zero() {
        let zero = FpReg::new(0, 0);
        assert_eq!(
            flags_of(|| {
                let _ = floatx80_div(80, RN, fx(1), zero);
            }),
            flag::DIVBYZERO
        );
        assert_eq!(
            flags_of(|| {
                let _ = floatx80_div(80, RN, zero, zero);
            }),
            flag::INVALID
        );
    }

    #[test]
    fn invalid_operations_raise_invalid() {
        // inf − inf, inf × 0, sqrt(negative).
        assert_eq!(
            flags_of(|| {
                let _ = floatx80_sub(80, RN, INF, INF);
            }),
            flag::INVALID
        );
        assert_eq!(
            flags_of(|| {
                let _ = floatx80_mul(80, RN, INF, FpReg::new(0, 0));
            }),
            flag::INVALID
        );
        assert_eq!(
            flags_of(|| {
                let _ = floatx80_sqrt(80, RN, fx(-1));
            }),
            flag::INVALID
        );
    }

    #[test]
    fn signaling_nan_input_raises_invalid_on_widen() {
        // Single-precision signalling NaN (exp 0xFF, quiet bit clear).
        assert_eq!(
            flags_of(|| {
                let _ = float32_to_floatx80(0x7FA0_0000);
            }),
            flag::INVALID
        );
        // Quiet NaN does not.
        assert_eq!(
            flags_of(|| {
                let _ = float32_to_floatx80(0x7FC0_0000);
            }),
            0
        );
    }

    #[test]
    fn integer_conversion_overflow_raises_invalid() {
        // 2^64-ish — far beyond i32 range.
        let big = FpReg::new(0x403F, 0x8000_0000_0000_0000);
        assert_eq!(
            flags_of(|| {
                let _ = floatx80_to_int32(RN, big);
            }),
            flag::INVALID
        );
        // A fractional value rounds inexactly.
        assert_eq!(
            flags_of(|| {
                let _ = floatx80_to_int32(RN, FpReg::new(0x3FFF, 0xC000_0000_0000_0000));
            }),
            flag::INEXACT
        );
    }

    // --- Packed-decimal conversion ---

    #[test]
    fn pack_decimal_specials_pass_through() {
        // +0 / −0: zero significand keeps only the sign.
        assert_eq!(pack_decimal_to_floatx80([0, 0, 0], RN), FpReg::new(0, 0));
        assert_eq!(
            pack_decimal_to_floatx80([0x8000_0000, 0, 0], RN),
            FpReg::new(0x8000, 0)
        );
        // +∞ copies through the extended layout (exponent field 0x7FFF).
        assert_eq!(
            pack_decimal_to_floatx80([0x7FFF_0000, 0, 0], RN),
            FpReg::new(0x7FFF, 0)
        );
        // NaN copies bit for bit.
        let nan = pack_decimal_to_floatx80([0x7FFF_0000, 0xC000_0000, 0x0000_0001], RN);
        assert_eq!(nan, FpReg::new(0x7FFF, 0xC000_0000_0000_0001));
    }

    #[test]
    fn pack_decimal_decodes_one() {
        // 1.0 = +1e+0: integer digit 1, zero fraction, exponent 0.
        let one = pack_decimal_to_floatx80([0x0000_0001, 0, 0], RN);
        assert_eq!(one, int32_to_floatx80(1));
    }

    #[test]
    fn pack_decimal_round_trips_small_integers() {
        // With the maximum 17-digit k-factor, exact integers survive a full
        // binary → BCD → binary round trip unchanged.
        for v in [1i32, 2, 7, 10, 100, 12_345, 1_000_000, -42, -987_654] {
            let bin = int32_to_floatx80(v);
            let bcd = floatx80_to_pack_decimal(bin, 17, RN);
            let back = pack_decimal_to_floatx80(bcd, RN);
            assert_eq!(back, bin, "round trip failed for {v}");
        }
    }

    #[test]
    fn pack_decimal_store_specials_pass_through() {
        // ∞ stores back to the extended layout (exponent field 0x7FFF).
        let inf = FpReg::new(0x7FFF, 0);
        assert_eq!(floatx80_to_pack_decimal(inf, 17, RN), [0x7FFF_0000, 0, 0]);
        // ±0 stores as a zero significand.
        assert_eq!(
            floatx80_to_pack_decimal(FpReg::new(0, 0), 5, RN)[1..],
            [0, 0]
        );
    }

    #[test]
    fn pack_decimal_inexact_sets_decimal_flag() {
        // 0.1 (1e−1) has no exact binary form → the conversion is inexact.
        let raised = flags_of(|| {
            let _ = pack_decimal_to_floatx80([0x4001_0001, 0, 0], RN);
        });
        assert_ne!(raised & flag::DECIMAL, 0, "expected INEX1/decimal flag");
    }
}
