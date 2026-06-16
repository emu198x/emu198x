//! MC68881/2 transcendental functions — a Rust port of WinUAE's
//! `softfloat_fpsp.cpp` (Andreas Grabher / Previous, derived from Motorola's
//! FPSP library). These are the `floatx80` transcendentals the 68881/2
//! compute in microcode: the exponentials, logarithms, trigonometric,
//! inverse-trigonometric, and hyperbolic functions.
//!
//! The algorithms are argument-reduction + polynomial / table evaluation
//! built entirely on the base `floatx80` ops in [`crate::softfloat`]. WinUAE's
//! `SET_PREC` / `RESET_PREC` force round-to-nearest at extended (80-bit)
//! precision for every *internal* step, restoring the user's rounding
//! precision and mode only for the *final* result-producing op. We mirror
//! that exactly: the internal-op helpers below pin (80, nearest-even); each
//! function threads the caller's `(precision, mode)` into its final op(s).
//!
//! Validated bit-exact against WinUAE's softfloat via
//! `validation/run_fpsp.sh` (the same differential oracle the base ops use).

use crate::registers::FpReg;
use crate::softfloat::{
    self, RoundingMode, exp, flag, float_raise, frac, normalize_floatx80_subnormal, pack,
    propagate_floatx80_nan_one_arg, sign,
};
use crate::softfloat_fpsp_tables::{
    ATAN_TBL, EXP_TBL, EXP_TBL2, EXP2_TBL, EXP2_TBL2, LOG_TBL, PI_TBL, PI_TBL2,
};

const ONE_EXP: i32 = 0x3FFF;
const ONE_SIG: u64 = 0x8000_0000_0000_0000;

/// Round-to-nearest-even — the rounding mode every *internal* FPSP step uses
/// (WinUAE `SET_PREC`).
const RN: RoundingMode = RoundingMode::NearestEven;

// Internal-op shorthands: extended precision, round-to-nearest. Each mirrors a
// `floatx80_*(…, status)` call made between SET_PREC and RESET_PREC.
#[inline]
fn mul(a: FpReg, b: FpReg) -> FpReg {
    softfloat::floatx80_mul(80, RN, a, b)
}
#[inline]
fn add(a: FpReg, b: FpReg) -> FpReg {
    softfloat::floatx80_add(80, RN, a, b)
}
#[inline]
fn sub(a: FpReg, b: FpReg) -> FpReg {
    softfloat::floatx80_sub(80, RN, a, b)
}
#[inline]
fn div(a: FpReg, b: FpReg) -> FpReg {
    softfloat::floatx80_div(80, RN, a, b)
}
#[inline]
fn to_i32(a: FpReg) -> i32 {
    softfloat::floatx80_to_int32(RN, a)
}
#[inline]
fn i32x(n: i32) -> FpReg {
    softfloat::int32_to_floatx80(n)
}
#[inline]
fn f32x(bits: u32) -> FpReg {
    softfloat::float32_to_floatx80(bits)
}
#[inline]
fn f64x(bits: u64) -> FpReg {
    softfloat::float64_to_floatx80(bits)
}

/// `floatx80_make_compact`: a sortable 32-bit key (exponent in the high word,
/// the significand's top 16 bits in the low word) for the range tests.
#[inline]
fn make_compact(a_exp: i32, a_sig: u64) -> i32 {
    (a_exp << 16) | (a_sig >> 48) as i32
}

/// True when `a` is a NaN (max exponent, non-zero significand beyond the
/// integer bit). The caller has already checked `a_exp == 0x7FFF`.
#[inline]
fn sig_is_nan(a_sig: u64) -> bool {
    a_sig.wrapping_shl(1) != 0
}

// ─── Exponentials ─────────────────────────────────────────────────────────

/// Shared tail of `floatx80_etox` (the C `expcont1` label): `n` = round(64/log2
/// · X), `m`/`m1` = biased scale exponents, `adjflag` selects the two-factor
/// scaling for the near-overflow range. `x` is the original argument.
fn etox_cont(
    precision: i32,
    mode: RoundingMode,
    x: FpReg,
    n: i32,
    m: i32,
    m1: i32,
    adjflag: bool,
) -> FpReg {
    let j = (n & 0x3F) as usize;
    let fp0n = i32x(n); // N as a float
    let fp1 = x; // X

    let mut fp0 = mul(fp0n, f32x(0xBC31_7218)); // N * L1   (L1 = lead(-log2/64))
    let l2 = pack(false, 0x3FDC, 0x82E3_0865_4361_C4C6);
    let fp2n = mul(fp0n, l2); // N * L2   (L1+L2 = -log2/64)
    fp0 = add(fp0, fp1); // X + N*L1
    fp0 = add(fp0, fp2n); // R

    let fp1s = mul(fp0, fp0); // S = R*R
    let mut fp2 = mul(f32x(0x3AB6_0B70), fp1s); // S*A5
    let mut fp3 = mul(f32x(0x3C08_8895), fp1s); // S*A4
    fp2 = add(fp2, f64x(0x3FA5_5555_5555_4431)); // A3 + S*A5
    fp3 = add(fp3, f64x(0x3FC5_5555_5555_4018)); // A2 + S*A4
    fp2 = mul(fp2, fp1s); // S*(A3+S*A5)
    fp3 = mul(fp3, fp1s); // S*(A2+S*A4)
    fp2 = add(fp2, f32x(0x3F00_0000)); // A1 + S*(A3+S*A5)
    fp3 = mul(fp3, fp0); // R*S*(A2+S*A4)
    fp2 = mul(fp2, fp1s); // S*(A1+S*(A3+S*A5))
    fp0 = add(fp0, fp3); // R + R*S*(A2+S*A4)
    fp0 = add(fp0, fp2); // EXP(R) - 1

    let tbl = EXP_TBL[j];
    fp0 = mul(fp0, tbl); // 2^(J/64)*(Exp(R)-1)
    fp0 = add(fp0, f32x(EXP_TBL2[j])); // accurate 2^(J/64)
    fp0 = add(fp0, tbl);

    let scale = pack(false, m, ONE_SIG);
    if adjflag {
        fp0 = mul(fp0, pack(false, m1, ONE_SIG));
    }

    let r = softfloat::floatx80_mul(precision, mode, fp0, scale);
    float_raise(flag::INEXACT);
    r
}

/// `floatx80_etox` (FETOX): e^x.
#[must_use]
pub fn floatx80_etox(precision: i32, mode: RoundingMode, a: FpReg) -> FpReg {
    let a_sig = frac(a);
    let a_exp = exp(a);
    let a_sign = sign(a);

    if a_exp == 0x7FFF {
        if sig_is_nan(a_sig) {
            return propagate_floatx80_nan_one_arg(a);
        }
        if a_sign {
            return pack(false, 0, 0); // e^-inf = +0
        }
        return a; // e^+inf = +inf
    }
    if a_exp == 0 && a_sig == 0 {
        return pack(false, ONE_EXP, ONE_SIG); // e^0 = 1
    }

    if a_exp >= 0x3FBE {
        // |X| >= 2^(-65)
        let compact = make_compact(a_exp, a_sig);
        if compact < 0x400C_B167 {
            // |X| < 16380 log2
            let fp0 = mul(a, f32x(0x42B8_AA3B)); // 64/log2 * X
            let n = to_i32(fp0);
            let j = n & 0x3F;
            let mut m = n / 64;
            if n < 0 && j != 0 {
                m -= 1;
            }
            m += 0x3FFF;
            etox_cont(precision, mode, a, n, m, 0, false)
        } else if compact > 0x400C_B27C {
            // |X| >= 16480 log2 — under/overflow
            let r = if a_sign {
                softfloat::round_and_pack_floatx80(precision, mode, false, -0x1000, a_sig, 0)
            } else {
                softfloat::round_and_pack_floatx80(precision, mode, false, 0x8000, a_sig, 0)
            };
            float_raise(flag::INEXACT);
            r
        } else {
            // 16380 log2 <= |X| < 16480 log2 — two-factor scaling
            let fp0 = mul(a, f32x(0x42B8_AA3B));
            let n = to_i32(fp0);
            let j = n & 0x3F;
            let mut k = n / 64;
            if n < 0 && j != 0 {
                k -= 1;
            }
            let mut m1 = k / 2;
            if k < 0 && (k & 1) != 0 {
                m1 -= 1;
            }
            let m = k - m1;
            etox_cont(precision, mode, a, n, m + 0x3FFF, m1 + 0x3FFF, true)
        }
    } else {
        // |X| < 2^(-65): e^x ≈ 1 + x
        let r = softfloat::floatx80_add(precision, mode, a, f32x(0x3F80_0000));
        float_raise(flag::INEXACT);
        r
    }
}

/// `floatx80_etoxm1` (FETOXM1): e^x − 1.
#[must_use]
pub fn floatx80_etoxm1(precision: i32, mode: RoundingMode, a: FpReg) -> FpReg {
    let a_sig = frac(a);
    let a_exp = exp(a);
    let a_sign = sign(a);

    if a_exp == 0x7FFF {
        if sig_is_nan(a_sig) {
            return propagate_floatx80_nan_one_arg(a);
        }
        if a_sign {
            return pack(true, ONE_EXP, ONE_SIG); // e^-inf - 1 = -1
        }
        return a;
    }
    if a_exp == 0 && a_sig == 0 {
        return pack(a_sign, 0, 0); // e^±0 - 1 = ±0
    }

    if a_exp >= 0x3FFD {
        // |X| >= 1/4
        let compact = make_compact(a_exp, a_sig);
        if compact <= 0x4004_C215 {
            // |X| <= 70 log2
            let fp0v = mul(a, f32x(0x42B8_AA3B)); // 64/log2 * X
            let n = to_i32(fp0v);
            let fp0n = i32x(n);
            let j = (n & 0x3F) as usize;
            let mut m = n / 64;
            if n < 0 && (n & 0x3F) != 0 {
                m -= 1;
            }
            let m1 = -m;

            let fp2n = fp0n; // N
            let mut fp0 = mul(fp0n, f32x(0xBC31_7218)); // N * L1
            let l2 = pack(false, 0x3FDC, 0x82E3_0865_4361_C4C6);
            let fp2 = mul(fp2n, l2); // N * L2
            fp0 = add(fp0, a); // X + N*L1
            fp0 = add(fp0, fp2); // R

            let fp1 = mul(fp0, fp0); // S = R*R
            let mut fp2 = mul(f32x(0x3950_097B), fp1); // S*A6
            let mut fp3 = mul(f32x(0x3AB6_0B6A), fp1); // S*A5
            fp2 = add(fp2, f64x(0x3F81_1111_1117_4385)); // A4 + S*A6
            fp3 = add(fp3, f64x(0x3FA5_5555_5555_4F5A)); // A3 + S*A5
            fp2 = mul(fp2, fp1); // S*(A4+S*A6)
            fp3 = mul(fp3, fp1); // S*(A3+S*A5)
            fp2 = add(fp2, f64x(0x3FC5_5555_5555_5555)); // A2 + S*(A4+S*A6)
            fp3 = add(fp3, f32x(0x3F00_0000)); // A1 + S*(A3+S*A5)
            fp2 = mul(fp2, fp1); // S*(A2+S*(A4+S*A6))
            let fp1 = mul(fp1, fp3); // S*(A1+S*(A3+S*A5))
            fp2 = mul(fp2, fp0); // R*S*(A2+S*(A4+S*A6))
            fp0 = add(fp0, fp1); // R + S*(A1+S*(A3+S*A5))
            fp0 = add(fp0, fp2); // EXP(R) - 1

            fp0 = mul(fp0, EXP_TBL[j]); // 2^(J/64)*(Exp(R)-1)

            let onebysc = pack(true, m1 + 0x3FFF, ONE_SIG); // -2^(-M)
            if m >= 64 {
                let fp1 = add(f32x(EXP_TBL2[j]), onebysc);
                fp0 = add(fp0, fp1);
                fp0 = add(fp0, EXP_TBL[j]);
            } else if m < -3 {
                fp0 = add(fp0, f32x(EXP_TBL2[j]));
                fp0 = add(fp0, EXP_TBL[j]);
                fp0 = add(fp0, onebysc);
            } else {
                let fp1 = add(EXP_TBL[j], onebysc);
                fp0 = add(fp0, f32x(EXP_TBL2[j]));
                fp0 = add(fp0, fp1);
            }

            let sc = pack(false, m + 0x3FFF, ONE_SIG);
            let r = softfloat::floatx80_mul(precision, mode, fp0, sc);
            float_raise(flag::INEXACT);
            r
        } else {
            // |X| > 70 log2
            if a_sign {
                let r =
                    softfloat::floatx80_add(precision, mode, f32x(0xBF80_0000), f32x(0x0080_0000)); // -1 + 2^(-126)
                float_raise(flag::INEXACT);
                r
            } else {
                floatx80_etox(precision, mode, a)
            }
        }
    } else if a_exp >= 0x3FBE {
        // 2^(-65) <= |X| < 1/4
        let fp0 = mul(a, a); // S = X*X
        let mut fp1 = mul(f32x(0x2F30_CAA8), fp0); // S*B12
        let mut fp2 = f32x(0x310F_8290); // B11
        fp1 = add(fp1, f32x(0x32D7_3220)); // B10
        fp2 = mul(fp2, fp0);
        fp1 = mul(fp1, fp0);
        fp2 = add(fp2, f32x(0x3493_F281)); // B9
        fp1 = add(fp1, f64x(0x3EC7_1DE3_A577_4682)); // B8
        fp2 = mul(fp2, fp0);
        fp1 = mul(fp1, fp0);
        fp2 = add(fp2, f64x(0x3EFA_01A0_19D7_CB68)); // B7
        fp1 = add(fp1, f64x(0x3F2A_01A0_1A01_9DF3)); // B6
        fp2 = mul(fp2, fp0);
        fp1 = mul(fp1, fp0);
        fp2 = add(fp2, f64x(0x3F56_C16C_16C1_70E2)); // B5
        fp1 = add(fp1, f64x(0x3F81_1111_1111_1111)); // B4
        fp2 = mul(fp2, fp0);
        fp1 = mul(fp1, fp0);
        fp2 = add(fp2, f64x(0x3FA5_5555_5555_5555)); // B3
        fp1 = add(fp1, pack(false, 0x3FFC, 0xAAAA_AAAA_AAAA_AAAB)); // B2
        fp2 = mul(fp2, fp0);
        fp1 = mul(fp1, fp0);

        fp2 = mul(fp2, fp0);
        fp1 = mul(fp1, a);

        let fp0 = mul(fp0, f32x(0x3F00_0000)); // S*B1
        let fp1 = add(fp1, fp2); // Q
        let fp0 = add(fp0, fp1); // S*B1 + Q

        let r = softfloat::floatx80_add(precision, mode, fp0, a);
        float_raise(flag::INEXACT);
        r
    } else {
        // |X| < 2^(-65)
        let sc = pack(true, 1, ONE_SIG);
        let r = if a_exp < 0x0033 {
            // |X| < 2^(-16382)
            let mut fp0 = mul(a, f64x(0x48B0_0000_0000_0000));
            fp0 = add(fp0, sc);
            softfloat::floatx80_mul(precision, mode, fp0, f64x(0x3730_0000_0000_0000))
        } else {
            softfloat::floatx80_add(precision, mode, a, sc)
        };
        float_raise(flag::INEXACT);
        r
    }
}

/// Shared tail of `floatx80_twotox` / `floatx80_tentox`: given the reduced
/// `fp0` (= R) and the table indices, evaluate the exp polynomial and scale.
/// `fact1`/`fact2`/`adjfact` are the precomputed scaling factors.
fn exp2_poly_scale(
    precision: i32,
    mode: RoundingMode,
    mut fp0: FpReg,
    fact1: FpReg,
    fact2: FpReg,
    adjfact: FpReg,
) -> FpReg {
    let fp1 = mul(fp0, fp0); // S = R*R
    let mut fp2 = mul(f64x(0x3F56_C16D_6F7B_D0B2), fp1); // S*A5
    let mut fp3 = mul(f64x(0x3F81_1112_302C_712C), fp1); // S*A4
    fp2 = add(fp2, f64x(0x3FA5_5555_5555_4CC1)); // A3 + S*A5
    fp3 = add(fp3, f64x(0x3FC5_5555_5555_4A54)); // A2 + S*A4
    fp2 = mul(fp2, fp1); // S*(A3+S*A5)
    fp3 = mul(fp3, fp1); // S*(A2+S*A4)
    fp2 = add(fp2, f64x(0x3FE0_0000_0000_0000)); // A1 + S*(A3+S*A5)
    fp3 = mul(fp3, fp0); // R*S*(A2+S*A4)
    fp2 = mul(fp2, fp1); // S*(A1+S*(A3+S*A5))
    fp0 = add(fp0, fp3); // R + R*S*(A2+S*A4)
    fp0 = add(fp0, fp2); // EXP(R) - 1

    fp0 = mul(fp0, fact1);
    fp0 = add(fp0, fact2);
    fp0 = add(fp0, fact1);

    let r = softfloat::floatx80_mul(precision, mode, fp0, adjfact);
    float_raise(flag::INEXACT);
    r
}

/// Build the `fact1`/`fact2` exp2 scaling factors from the table and the
/// integer scale `m` (mirrors the `fact1.high += m` / `fact2` shuffle).
fn exp2_factors(j: usize, m: i32) -> (FpReg, FpReg) {
    let mut fact1 = EXP2_TBL[j];
    fact1.high = (i32::from(fact1.high) + m) as u16;
    let t2 = EXP2_TBL2[j];
    let fact2 = FpReg::new(
        ((t2 >> 16) as i32 + m) as u16,
        (u64::from(t2 & 0xFFFF)) << 48,
    );
    (fact1, fact2)
}

/// `floatx80_twotox` (FTWOTOX): 2^x.
#[must_use]
pub fn floatx80_twotox(precision: i32, mode: RoundingMode, a: FpReg) -> FpReg {
    let a_sig = frac(a);
    let a_exp = exp(a);
    let a_sign = sign(a);

    if a_exp == 0x7FFF {
        if sig_is_nan(a_sig) {
            return propagate_floatx80_nan_one_arg(a);
        }
        if a_sign {
            return pack(false, 0, 0);
        }
        return a;
    }
    if a_exp == 0 && a_sig == 0 {
        return pack(false, ONE_EXP, ONE_SIG);
    }

    let compact = make_compact(a_exp, a_sig);
    if !(0x3FB9_8000..=0x400D_80C0).contains(&compact) {
        // |X| > 16480 or |X| < 2^(-70)
        if compact > 0x3FFF_8000 {
            let r = if a_sign {
                softfloat::round_and_pack_floatx80(precision, mode, false, -0x1000, a_sig, 0)
            } else {
                softfloat::round_and_pack_floatx80(precision, mode, false, 0x8000, a_sig, 0)
            };
            return r;
        }
        let r = softfloat::floatx80_add(precision, mode, a, f32x(0x3F80_0000)); // 1 + X
        float_raise(flag::INEXACT);
        return r;
    }

    // 2^(-70) <= |X| <= 16480
    let fp1m = mul(a, f32x(0x4280_0000)); // X * 64
    let n = to_i32(fp1m);
    let fp1 = i32x(n);
    let j = (n & 0x3F) as usize;
    let mut l = n / 64;
    if n < 0 && (n & 0x3F) != 0 {
        l -= 1;
    }
    let mut m = l / 2;
    if l < 0 && (l & 1) != 0 {
        m -= 1;
    }
    let m1 = l - m + 0x3FFF;
    let adjfact = pack(false, m1, ONE_SIG);
    let (fact1, fact2) = exp2_factors(j, m);

    let fp1 = mul(fp1, f32x(0x3C80_0000)); // (1/64)*N
    let mut fp0 = sub(a, fp1); // X - (1/64)*INT(64 X)
    fp0 = mul(fp0, pack(false, 0x3FFE, 0xB172_17F7_D1CF_79AC)); // R = (X-...) * LOG2

    exp2_poly_scale(precision, mode, fp0, fact1, fact2, adjfact)
}

/// `floatx80_tentox` (FTENTOX): 10^x.
#[must_use]
pub fn floatx80_tentox(precision: i32, mode: RoundingMode, a: FpReg) -> FpReg {
    let a_sig = frac(a);
    let a_exp = exp(a);
    let a_sign = sign(a);

    if a_exp == 0x7FFF {
        if sig_is_nan(a_sig) {
            return propagate_floatx80_nan_one_arg(a);
        }
        if a_sign {
            return pack(false, 0, 0);
        }
        return a;
    }
    if a_exp == 0 && a_sig == 0 {
        return pack(false, ONE_EXP, ONE_SIG);
    }

    let compact = make_compact(a_exp, a_sig);
    if !(0x3FB9_8000..=0x400B_9B07).contains(&compact) {
        // |X| > 16480 LOG2/LOG10 or |X| < 2^(-70)
        if compact > 0x3FFF_8000 {
            let r = if a_sign {
                softfloat::round_and_pack_floatx80(precision, mode, false, -0x1000, a_sig, 0)
            } else {
                softfloat::round_and_pack_floatx80(precision, mode, false, 0x8000, a_sig, 0)
            };
            return r;
        }
        let r = softfloat::floatx80_add(precision, mode, a, f32x(0x3F80_0000)); // 1 + X
        float_raise(flag::INEXACT);
        return r;
    }

    // 2^(-70) <= |X| <= 16480 LOG2/LOG10
    let fp1m = mul(a, f64x(0x406A_934F_0979_A371)); // X*64*LOG10/LOG2
    let n = to_i32(fp1m);
    let fp1 = i32x(n);
    let j = (n & 0x3F) as usize;
    let mut l = n / 64;
    if n < 0 && (n & 0x3F) != 0 {
        l -= 1;
    }
    let mut m = l / 2;
    if l < 0 && (l & 1) != 0 {
        m -= 1;
    }
    let m1 = l - m + 0x3FFF;
    let adjfact = pack(false, m1, ONE_SIG);
    let (fact1, fact2) = exp2_factors(j, m);

    let fp2n = fp1; // N
    let fp1 = mul(fp1, f64x(0x3F73_4413_509F_8000)); // N*(LOG2/64LOG10)_LEAD
    let fp2 = mul(fp2n, pack(true, 0x3FCD, 0xC021_9DC1_DA99_4FD2)); // N*(LOG2/64LOG10)_TRAIL
    let mut fp0 = sub(a, fp1); // X - N L_LEAD
    fp0 = sub(fp0, fp2); // X - N L_TRAIL
    fp0 = mul(fp0, pack(false, 0x4000, 0x935D_8DDD_AAA8_AC17)); // R = (…) * LOG10

    exp2_poly_scale(precision, mode, fp0, fact1, fact2, adjfact)
}

// ─── Logarithms ─────────────────────────────────────────────────────────────

/// The 68881/2 default (created) NaN — sign clear, max exponent, all-ones
/// significand (WinUAE `floatx80_default_nan`).
#[inline]
fn default_nan() -> FpReg {
    FpReg::new(0x7FFF, 0xFFFF_FFFF_FFFF_FFFF)
}

/// −∞ (WinUAE `packFloatx80(1, 0x7FFF, floatx80_default_infinity_low)` — the
/// 68881/2 infinity has a zero significand / clear integer bit).
#[inline]
fn neg_inf() -> FpReg {
    pack(true, 0x7FFF, 0)
}

/// Shared `LP1CONT1` body of `logn` / `lognp1`: given `ymf` = Y−F, the
/// reduced fraction, `k_float` = the scale K as a float, and the table index
/// `j`, evaluate the log polynomial and add K·log2. Final op at the caller's
/// precision/mode.
fn log_cont1(precision: i32, mode: RoundingMode, ymf: FpReg, k_float: FpReg, j: usize) -> FpReg {
    let mut fp0 = mul(ymf, LOG_TBL[j]); // U = (Y-F)/F
    let logof2 = pack(false, 0x3FFE, 0xB172_17F7_D1CF_79AC);
    let klog2 = mul(k_float, logof2); // K*LOG2
    let fp2v = mul(fp0, fp0); // V = U*U
    let fp3 = fp2v;

    let mut fp1 = mul(fp2v, f64x(0x3FC2_499A_B5E4_040B)); // V*A6
    let mut fp2 = mul(fp2v, f64x(0xBFC5_55B5_848C_B7DB)); // V*A5
    fp1 = add(fp1, f64x(0x3FC9_9999_987D_8730)); // A4 + V*A6
    fp2 = add(fp2, f64x(0xBFCF_FFFF_FF6F_7E97)); // A3 + V*A5
    fp1 = mul(fp1, fp3); // V*(A4+V*A6)
    fp2 = mul(fp2, fp3); // V*(A3+V*A5)
    fp1 = add(fp1, f64x(0x3FD5_5555_5555_55A4)); // A2 + V*(A4+V*A6)
    fp2 = add(fp2, f64x(0xBFE0_0000_0000_0008)); // A1 + V*(A3+V*A5)
    fp1 = mul(fp1, fp3); // V*(A2+V*(A4+V*A6))
    fp2 = mul(fp2, fp3); // V*(A1+V*(A3+V*A5))
    fp1 = mul(fp1, fp0); // U*V*(A2+V*(A4+V*A6))
    fp0 = add(fp0, fp2); // U + V*(A1+V*(A3+V*A5))

    fp1 = add(fp1, LOG_TBL[j + 1]); // LOG(F) + U*V*(A2+V*(A4+V*A6))
    fp0 = add(fp0, fp1); // LOG(F) + LOG(1+U)

    let r = softfloat::floatx80_add(precision, mode, fp0, klog2);
    float_raise(flag::INEXACT);
    r
}

/// Shared `LP1CONT2` body: `two_num` = 2·(X−1) (or 2Z), `denom` = X+1 (or
/// 1+X). Computes U = two_num/denom and the odd-polynomial log. Final op at
/// the caller's precision/mode.
fn log_cont2(precision: i32, mode: RoundingMode, two_num: FpReg, denom: FpReg) -> FpReg {
    let saveu = div(two_num, denom); // U
    let mut fp0 = mul(saveu, saveu); // V = U*U
    let mut fp1 = mul(fp0, fp0); // W = V*V

    let mut fp3 = mul(f64x(0x3F17_5496_ADD7_DAD6), fp1); // W*B5
    let mut fp2 = mul(f64x(0x3F3C_71C2_FE80_C7E0), fp1); // W*B4
    fp3 = add(fp3, f64x(0x3F62_4924_928B_CCFF)); // B3 + W*B5
    fp2 = add(fp2, f64x(0x3F89_9999_9999_95EC)); // B2 + W*B4
    fp1 = mul(fp1, fp3); // W*(B3+W*B5)
    fp2 = mul(fp2, fp0); // V*(B2+W*B4)
    fp1 = add(fp1, f64x(0x3FB5_5555_5555_5555)); // B1 + W*(B3+W*B5)

    fp0 = mul(fp0, saveu); // U*V
    fp1 = add(fp1, fp2); // [B1+W*(B3+W*B5)] + [V*(B2+W*B4)]
    fp0 = mul(fp0, fp1);

    let r = softfloat::floatx80_add(precision, mode, fp0, saveu);
    float_raise(flag::INEXACT);
    r
}

/// `floatx80_logn` (FLOGN): natural logarithm.
#[must_use]
pub fn floatx80_logn(precision: i32, mode: RoundingMode, a: FpReg) -> FpReg {
    let mut a_sig = frac(a);
    let mut a_exp = exp(a);
    let a_sign = sign(a);

    if a_exp == 0x7FFF {
        if sig_is_nan(a_sig) {
            return propagate_floatx80_nan_one_arg(a);
        }
        if !a_sign {
            return a;
        }
    }

    let mut a = a;
    let mut adjk = 0;
    if a_exp == 0 {
        if a_sig == 0 {
            float_raise(flag::DIVBYZERO);
            return neg_inf();
        }
        if a_sig & ONE_SIG == 0 {
            // denormal: normalize and bias the eventual K by -100
            let (e, s) = normalize_floatx80_subnormal(a_sig);
            a_exp = e + 100;
            a_sig = s;
            adjk = -100;
            a = pack(a_sign, a_exp, a_sig);
        }
    }
    if a_sign {
        float_raise(flag::INVALID);
        return default_nan();
    }

    let compact = make_compact(a_exp, a_sig);
    if !(0x3FFE_F07D..=0x3FFF_8841).contains(&compact) {
        // |X-1| >= 1/16: argument reduction against the 1/F table
        let k = (a_exp - 0x3FFF) + adjk;
        let fsig = (a_sig & 0xFE00_0000_0000_0000) | 0x0100_0000_0000_0000;
        let j = ((fsig >> 56) & 0x7E) as usize;
        let f = pack(false, 0x3FFF, fsig); // F
        let y = pack(false, 0x3FFF, a_sig); // Y
        let ymf = sub(y, f); // Y - F
        log_cont1(precision, mode, ymf, i32x(k), j)
    } else {
        // |X-1| < 1/16: the odd-polynomial path
        let fp1 = sub(a, f32x(0x3F80_0000)); // X-1
        let fp0 = add(a, f32x(0x3F80_0000)); // X+1
        let fp1 = add(fp1, fp1); // 2(X-1)
        log_cont2(precision, mode, fp1, fp0)
    }
}

/// `floatx80_log10` (FLOG10): base-10 logarithm.
#[must_use]
pub fn floatx80_log10(precision: i32, mode: RoundingMode, a: FpReg) -> FpReg {
    let a_sig = frac(a);
    let a_exp = exp(a);
    let a_sign = sign(a);

    if a_exp == 0x7FFF {
        if sig_is_nan(a_sig) {
            return propagate_floatx80_nan_one_arg(a);
        }
        if !a_sign {
            return a;
        }
    }
    if a_exp == 0 && a_sig == 0 {
        float_raise(flag::DIVBYZERO);
        return neg_inf();
    }
    if a_sign {
        float_raise(flag::INVALID);
        return default_nan();
    }

    let fp0 = floatx80_logn(80, RN, a);
    let inv_l10 = pack(false, 0x3FFD, 0xDE5B_D8A9_3728_7195);
    let r = softfloat::floatx80_mul(precision, mode, fp0, inv_l10); // LOGN(X)*INV_L10
    float_raise(flag::INEXACT);
    r
}

/// `floatx80_log2` (FLOG2): base-2 logarithm.
#[must_use]
pub fn floatx80_log2(precision: i32, mode: RoundingMode, a: FpReg) -> FpReg {
    let mut a_sig = frac(a);
    let mut a_exp = exp(a);
    let a_sign = sign(a);

    if a_exp == 0x7FFF {
        if sig_is_nan(a_sig) {
            return propagate_floatx80_nan_one_arg(a);
        }
        if !a_sign {
            return a;
        }
    }
    if a_exp == 0 {
        if a_sig == 0 {
            float_raise(flag::DIVBYZERO);
            return neg_inf();
        }
        let (e, s) = normalize_floatx80_subnormal(a_sig);
        a_exp = e;
        a_sig = s;
    }
    if a_sign {
        float_raise(flag::INVALID);
        return default_nan();
    }

    let r = if a_sig == ONE_SIG {
        // X is exactly 2^k
        i32x(a_exp - 0x3FFF)
    } else {
        let fp0 = floatx80_logn(80, RN, a);
        let inv_l2 = pack(false, 0x3FFF, 0xB8AA_3B29_5C17_F0BC);
        softfloat::floatx80_mul(precision, mode, fp0, inv_l2) // LOGN(X)*INV_L2
    };
    float_raise(flag::INEXACT);
    r
}

/// `floatx80_lognp1` (FLOGNP1): natural logarithm of 1 + x.
#[must_use]
pub fn floatx80_lognp1(precision: i32, mode: RoundingMode, a: FpReg) -> FpReg {
    let a_sig = frac(a);
    let a_exp = exp(a);
    let a_sign = sign(a);

    if a_exp == 0x7FFF {
        if sig_is_nan(a_sig) {
            return propagate_floatx80_nan_one_arg(a);
        }
        if a_sign {
            float_raise(flag::INVALID);
            return default_nan();
        }
        return a;
    }
    if a_exp == 0 && a_sig == 0 {
        return pack(a_sign, 0, 0); // ln(1±0) = ±0
    }
    if a_sign && a_exp >= ONE_EXP {
        // x <= -1
        if a_exp == ONE_EXP && a_sig == ONE_SIG {
            float_raise(flag::DIVBYZERO); // ln(1 + -1) = ln(0) = -inf
            return pack(a_sign, 0x7FFF, 0);
        }
        float_raise(flag::INVALID); // ln of a negative
        return default_nan();
    }
    if a_exp < 0x3F99 || (a_exp == 0x3F99 && a_sig == ONE_SIG) {
        // |x| below the threshold: ln(1+x) ≈ x
        float_raise(flag::INEXACT);
        return softfloat::floatx80_move(precision, mode, a);
    }

    // X = 1 + Z
    let z = a;
    let x = add(a, f32x(0x3F80_0000));
    let x_exp = exp(x);
    let x_sig = frac(x);
    let compact = make_compact(x_exp, x_sig);

    if !(0x3FFE_8000..=0x3FFF_C000).contains(&compact) {
        // |X| < 1/2 or |X| > 3/2
        let k = x_exp - 0x3FFF;
        let fsig = (x_sig & 0xFE00_0000_0000_0000) | 0x0100_0000_0000_0000;
        let j = ((fsig >> 56) & 0x7E) as usize;
        let f = pack(false, 0x3FFF, fsig);
        let y = pack(false, 0x3FFF, x_sig);
        let ymf = sub(y, f);
        log_cont1(precision, mode, ymf, i32x(k), j)
    } else if !(0x3FFE_F07D..=0x3FFF_8841).contains(&compact) {
        // LP1CARE: |X-1| in [1/16, …] but |X| near 1
        let fsig = (x_sig & 0xFE00_0000_0000_0000) | 0x0100_0000_0000_0000;
        let f = pack(false, 0x3FFF, fsig);
        let j = ((fsig >> 56) & 0x7E) as usize;
        let (ymf, k_float) = if compact >= 0x3FFF_8000 {
            // KISZERO: 1+Z >= 1
            let fp0 = sub(f32x(0x3F80_0000), f); // 1 - F
            (add(fp0, z), pack(false, 0, 0)) // (1-F)+Z, K=0
        } else {
            // KISNEG: 1+Z < 1
            let fp0 = sub(f32x(0x4000_0000), f); // 2 - F
            let twoz = add(z, z); // 2Z
            (add(fp0, twoz), pack(true, ONE_EXP, ONE_SIG)) // (2-F)+2Z, K=-1
        };
        log_cont1(precision, mode, ymf, k_float, j)
    } else {
        // LP1ONE16: |X-1| < 1/16
        let twoz = add(z, z); // 2Z
        let denom = add(x, f32x(0x3F80_0000)); // 2 + Z
        log_cont2(precision, mode, twoz, denom)
    }
}

// ─── Trigonometric (sin / cos / sincos) ─────────────────────────────────────

/// `REDUCEX`: reduce a large argument (|X| ≥ 15π) modulo 2π, returning the
/// integer octant count `n` and the reduced `R`. Mirrors the C `loop`.
fn reducex(mut fp0: FpReg, compact: i32, a_sign: bool) -> (i32, FpReg) {
    let mut fp1 = pack(false, 0, 0);
    if compact == 0x7FFE_FFFF {
        let twopi1 = pack(!a_sign, 0x7FFE, 0xC90F_DAA2_0000_0000);
        let twopi2 = pack(!a_sign, 0x7FDC, 0x85A3_08D3_0000_0000);
        fp0 = add(fp0, twopi1);
        fp1 = fp0;
        fp0 = add(fp0, twopi2);
        fp1 = sub(fp1, fp0);
        fp1 = add(fp1, twopi2);
    }
    loop {
        let x_sign = sign(fp0);
        let x_exp = exp(fp0) - 0x3FFF;
        let (l, endflag) = if x_exp <= 28 {
            (0, true)
        } else {
            (x_exp - 27, false)
        };
        let invtwopi = pack(false, 0x3FFE - l, 0xA2F9_836E_4E44_152A);
        let twopi1 = pack(false, 0x3FFF + l, 0xC90F_DAA2_0000_0000);
        let twopi2 = pack(false, 0x3FDD + l, 0x85A3_08D3_0000_0000);
        let twoto63 = 0x5F00_0000u32 | if x_sign { 0x8000_0000 } else { 0 };

        let mut fp2 = mul(fp0, invtwopi);
        fp2 = add(fp2, f32x(twoto63));
        fp2 = sub(fp2, f32x(twoto63)); // FP2 is N
        let fp4n = mul(twopi1, fp2); // W = N*P1
        let fp5 = mul(twopi2, fp2); // w = N*P2
        let mut fp3 = add(fp4n, fp5); // P
        let mut fp4 = sub(fp4n, fp3); // W-P
        fp0 = sub(fp0, fp3); // A := R - P
        fp4 = add(fp4, fp5); // p = (W-P)+w
        fp3 = fp0; // A
        fp1 = sub(fp1, fp4); // a := r - p
        fp0 = add(fp0, fp1); // R := A+a
        if endflag {
            return (to_i32(fp2), fp0);
        }
        fp3 = sub(fp3, fp0); // A-R
        fp1 = add(fp1, fp3); // r := (A-R)+a
    }
}

/// `COSPOLY`: the cosine polynomial branch of the sin/cos kernel.
fn cospoly(precision: i32, mode: RoundingMode, r: FpReg, n: i32, adjn: i32) -> FpReg {
    let s = mul(r, r); // S
    let t = mul(s, s); // T
    let mut x_sign = sign(s);
    let x_exp = exp(s);
    let x_sig = frac(s);
    let posneg1: u32 = if ((n + adjn) >> 1) & 1 != 0 {
        x_sign = !x_sign;
        0xBF80_0000 // -1
    } else {
        0x3F80_0000 // 1
    };

    let mut fp2 = mul(f64x(0x3D2A_C4D0_D601_1EE3), t); // TB8
    let mut fp3 = mul(f64x(0xBDA9_396F_9F45_AC19), t); // TB7
    fp2 = add(fp2, f64x(0x3E21_EED9_0612_C972)); // B6+TB8
    fp3 = add(fp3, f64x(0xBE92_7E4F_B79D_9FCF)); // B5+TB7
    fp2 = mul(fp2, t); // T(B6+TB8)
    fp3 = mul(fp3, t); // T(B5+TB7)
    fp2 = add(fp2, f64x(0x3EFA_01A0_1A01_D423)); // B4+T(B6+TB8)
    fp3 = add(fp3, pack(true, 0x3FF5, 0xB60B_60B6_0B61_D438)); // B3+T(B5+TB7)
    fp2 = mul(fp2, t); // T(B4+T(B6+TB8))
    let mut fp1 = mul(t, fp3); // T(B3+T(B5+TB7))
    fp2 = add(fp2, pack(false, 0x3FFA, 0xAAAA_AAAA_AAAA_AB5E)); // B2+T(B4+T(B6+TB8))
    fp1 = add(fp1, f32x(0xBF00_0000)); // B1+T(B3+T(B5+TB7))
    let mut fp0 = mul(s, fp2); // S(B2+T(B4+T(B6+TB8)))
    fp0 = add(fp0, fp1);

    let x = pack(x_sign, x_exp, x_sig);
    fp0 = mul(fp0, x);
    let res = softfloat::floatx80_add(precision, mode, fp0, f32x(posneg1));
    float_raise(flag::INEXACT);
    res
}

/// `SINPOLY`: the sine polynomial branch of the sin/cos kernel.
fn sinpoly(precision: i32, mode: RoundingMode, r: FpReg, n: i32, adjn: i32) -> FpReg {
    let mut x_sign = sign(r);
    let x_exp = exp(r);
    let x_sig = frac(r);
    if ((n + adjn) >> 1) & 1 != 0 {
        x_sign = !x_sign;
    }

    let s = mul(r, r); // S
    let t = mul(s, s); // T
    let mut fp3 = mul(f64x(0xBD6A_AA77_CCC9_94F5), t); // T*A7
    let mut fp2 = mul(f64x(0x3DE6_1209_7AAE_8DA1), t); // T*A6
    fp3 = add(fp3, f64x(0xBE5A_E645_2A11_8AE4)); // A5+T*A7
    fp2 = add(fp2, f64x(0x3EC7_1DE3_A534_1531)); // A4+T*A6
    fp3 = mul(fp3, t); // T(A5+TA7)
    fp2 = mul(fp2, t); // T(A4+TA6)
    fp3 = add(fp3, f64x(0xBF2A_01A0_1A01_8B59)); // A3+T(A5+TA7)
    fp2 = add(fp2, pack(false, 0x3FF8, 0x8888_8888_8888_59AF)); // A2+T(A4+TA6)
    let mut fp1 = mul(t, fp3); // T(A3+T(A5+TA7))
    fp2 = mul(fp2, s); // S(A2+T(A4+TA6))
    fp1 = add(fp1, pack(true, 0x3FFC, 0xAAAA_AAAA_AAAA_AA99)); // A1+T(A3+T(A5+TA7))
    fp1 = add(fp1, fp2);

    let x = pack(x_sign, x_exp, x_sig);
    let mut fp0 = mul(s, x); // R'*S
    fp0 = mul(fp0, fp1); // SIN(R')-R'
    let res = softfloat::floatx80_add(precision, mode, fp0, x);
    float_raise(flag::INEXACT);
    res
}

/// Shared sin/cos kernel: argument-reduce `a` modulo π/2 (table) or 2π
/// (REDUCEX for large args) and evaluate the sine (`adjn` = 0) or cosine
/// (`adjn` = 1) polynomial. The NaN / zero special cases are handled by the
/// public wrappers.
fn sin_core(precision: i32, mode: RoundingMode, a: FpReg, adjn: i32) -> FpReg {
    let compact = make_compact(exp(a), frac(a));
    let (n, r) = if !(0x3FD7_8000..=0x4004_BC7E).contains(&compact) {
        if compact > 0x3FFF_8000 {
            reducex(a, compact, sign(a)) // |X| >= 15π
        } else {
            // SINSM: |X| < 2^(-40)
            let res = if adjn != 0 {
                // COSTINY: 1 - 2^(-126)
                softfloat::floatx80_sub(precision, mode, f32x(0x3F80_0000), f32x(0x0080_0000))
            } else {
                // SINTINY: x
                softfloat::floatx80_move(precision, mode, a)
            };
            float_raise(flag::INEXACT);
            return res;
        }
    } else {
        // Moderate: reduce modulo π/2 against the table.
        let fp1 = mul(a, f64x(0x3FE4_5F30_6DC9_C883)); // X*2/PI
        let n = to_i32(fp1);
        let j = (32 + n) as usize;
        let r = sub(sub(a, PI_TBL[j]), f32x(PI_TBL2[j])); // R = (X-Y1)-Y2
        (n, r)
    };

    if (n + adjn) & 1 != 0 {
        cospoly(precision, mode, r, n, adjn)
    } else {
        sinpoly(precision, mode, r, n, adjn)
    }
}

/// `floatx80_sin` (FSIN): sine.
#[must_use]
pub fn floatx80_sin(precision: i32, mode: RoundingMode, a: FpReg) -> FpReg {
    let a_sig = frac(a);
    let a_exp = exp(a);
    if a_exp == 0x7FFF {
        if sig_is_nan(a_sig) {
            return propagate_floatx80_nan_one_arg(a);
        }
        float_raise(flag::INVALID); // sin(±inf) is invalid
        return default_nan();
    }
    if a_exp == 0 && a_sig == 0 {
        return pack(sign(a), 0, 0); // sin(±0) = ±0
    }
    sin_core(precision, mode, a, 0)
}

/// `floatx80_cos` (FCOS): cosine.
#[must_use]
pub fn floatx80_cos(precision: i32, mode: RoundingMode, a: FpReg) -> FpReg {
    let a_sig = frac(a);
    let a_exp = exp(a);
    if a_exp == 0x7FFF {
        if sig_is_nan(a_sig) {
            return propagate_floatx80_nan_one_arg(a);
        }
        float_raise(flag::INVALID); // cos(±inf) is invalid
        return default_nan();
    }
    if a_exp == 0 && a_sig == 0 {
        return pack(false, ONE_EXP, ONE_SIG); // cos(±0) = 1
    }
    sin_core(precision, mode, a, 1)
}

/// Reduce a moderate argument modulo π/2 against the table, returning the
/// octant `n` and reduced `R` (shared by sin/cos and tan).
fn reduce_moderate(a: FpReg) -> (i32, FpReg) {
    let fp1 = mul(a, f64x(0x3FE4_5F30_6DC9_C883)); // X*2/PI
    let n = to_i32(fp1);
    let j = (32 + n) as usize;
    (n, sub(sub(a, PI_TBL[j]), f32x(PI_TBL2[j]))) // R = (X-Y1)-Y2
}

/// `tancont`: the tangent rational-approximation tail. `n` (octant) selects
/// the odd branch (tan = 1/−cot) or the even branch (tan = P/Q).
fn tancont(precision: i32, mode: RoundingMode, r: FpReg, n: i32) -> FpReg {
    let q4 = f64x(0x3EA0_B759_F50F_8688);
    let p3 = f64x(0xBEF2_BAA5_A892_4F04);
    let q3 = f64x(0xBF34_6F59_B39B_A65F);
    let p2 = pack(false, 0x3FF6, 0xE073_D3FC_199C_4A00);
    let q2 = pack(false, 0x3FF9, 0xD23C_D684_15D9_5FA1);
    let p1 = pack(true, 0x3FFC, 0x8895_A6C5_FB42_3BCA);
    let q1 = pack(true, 0x3FFD, 0xEEF5_7E0D_A84B_C8CE);
    let one = f32x(0x3F80_0000);

    let s = mul(r, r); // S = R*R
    let mut fp3 = mul(q4, s); // SQ4
    let mut fp2 = mul(p3, s); // SP3
    fp3 = add(fp3, q3); // Q3+SQ4
    fp2 = add(fp2, p2); // P2+SP3
    fp3 = mul(fp3, s); // S(Q3+SQ4)
    fp2 = mul(fp2, s); // S(P2+SP3)
    fp3 = add(fp3, q2); // Q2+S(Q3+SQ4)
    fp2 = add(fp2, p1); // P1+S(P2+SP3)
    fp3 = mul(fp3, s); // S(Q2+S(Q3+SQ4))
    fp2 = mul(fp2, s); // S(P1+S(P2+SP3))
    fp3 = add(fp3, q1); // Q1+S(Q2+S(Q3+SQ4))
    fp2 = mul(fp2, r); // RS(P1+S(P2+SP3))

    let qpoly = add(mul(s, fp3), one); // 1 + S(Q1+…)
    let ppoly = add(r, fp2); // R + RS(P1+…)

    let res = if n & 1 != 0 {
        // NODD: tan = (1+SQ) / −(R+RSP)
        let den = pack(!sign(ppoly), exp(ppoly), frac(ppoly));
        softfloat::floatx80_div(precision, mode, qpoly, den)
    } else {
        // NEVEN: tan = (R+RSP) / (1+SQ)
        softfloat::floatx80_div(precision, mode, ppoly, qpoly)
    };
    float_raise(flag::INEXACT);
    res
}

/// `floatx80_tan` (FTAN): tangent.
#[must_use]
pub fn floatx80_tan(precision: i32, mode: RoundingMode, a: FpReg) -> FpReg {
    let a_sig = frac(a);
    let a_exp = exp(a);
    if a_exp == 0x7FFF {
        if sig_is_nan(a_sig) {
            return propagate_floatx80_nan_one_arg(a);
        }
        float_raise(flag::INVALID);
        return default_nan();
    }
    if a_exp == 0 && a_sig == 0 {
        return pack(sign(a), 0, 0);
    }

    let compact = make_compact(a_exp, a_sig);
    let (n, r) = if !(0x3FD7_8000..=0x4004_BC7E).contains(&compact) {
        if compact > 0x3FFF_8000 {
            reducex(a, compact, sign(a))
        } else {
            // tiny: tan(x) ≈ x
            let res = softfloat::floatx80_move(precision, mode, a);
            float_raise(flag::INEXACT);
            return res;
        }
    } else {
        reduce_moderate(a)
    };
    tancont(precision, mode, r, n)
}

/// `floatx80_sincos` (FSINCOS): returns `(sin, cos)` of `a`, computed with a
/// single shared argument reduction and interleaved sine/cosine polynomials.
#[must_use]
pub fn floatx80_sincos(precision: i32, mode: RoundingMode, a: FpReg) -> (FpReg, FpReg) {
    let a_sig = frac(a);
    let a_exp = exp(a);
    if a_exp == 0x7FFF {
        if sig_is_nan(a_sig) {
            let n = propagate_floatx80_nan_one_arg(a);
            return (n, n);
        }
        float_raise(flag::INVALID);
        let n = default_nan();
        return (n, n);
    }
    if a_exp == 0 && a_sig == 0 {
        return (pack(sign(a), 0, 0), pack(false, ONE_EXP, ONE_SIG)); // (±0, 1)
    }

    let compact = make_compact(a_exp, a_sig);
    let (n, r) = if !(0x3FD7_8000..=0x4004_BC7E).contains(&compact) {
        if compact > 0x3FFF_8000 {
            reducex(a, compact, sign(a))
        } else {
            // SCSM (tiny): cos = 1 - 2^(-126), sin = x
            let cos =
                softfloat::floatx80_sub(precision, mode, f32x(0x3F80_0000), f32x(0x0080_0000));
            let sin = softfloat::floatx80_move(precision, mode, a);
            float_raise(flag::INEXACT);
            return (sin, cos);
        }
    } else {
        reduce_moderate(a)
    };

    let n = n & 3; // k = N mod 4
    if n & 1 != 0 {
        // NODD
        let j1 = n >> 1;
        let j2 = j1 ^ (n & 1);
        let mut r_sign = sign(r);
        let r_exp = exp(r);
        let r_sig = frac(r);
        r_sign ^= j2 != 0;

        let s = mul(r, r); // S = R*R
        let mut fp1 = mul(f64x(0xBD6A_AA77_CCC9_94F5), s); // SA7
        let mut fp2 = mul(f64x(0x3D2A_C4D0_D601_1EE3), s); // SB8
        fp1 = add(fp1, f64x(0x3DE6_1209_7AAE_8DA1)); // A6+SA7
        fp2 = add(fp2, f64x(0xBDA9_396F_9F45_AC19)); // B7+SB8
        fp1 = mul(fp1, s);
        fp2 = mul(fp2, s);
        fp1 = add(fp1, f64x(0xBE5A_E645_2A11_8AE4)); // A5+S(A6+SA7)
        fp2 = add(fp2, f64x(0x3E21_EED9_0612_C972)); // B6+S(B7+SB8)
        fp1 = mul(fp1, s);
        fp2 = mul(fp2, s);

        let mut s_sign = sign(s);
        let s_exp = exp(s);
        let s_sig = frac(s);
        s_sign ^= j1 != 0;
        let posneg1 = 0x3F80_0000u32 | if j1 != 0 { 0x8000_0000 } else { 0 };

        fp1 = add(fp1, f64x(0x3EC7_1DE3_A534_1531)); // A4+…
        fp2 = add(fp2, f64x(0xBE92_7E4F_B79D_9FCF)); // B5+…
        fp1 = mul(fp1, s);
        fp2 = mul(fp2, s);
        fp1 = add(fp1, f64x(0xBF2A_01A0_1A01_8B59)); // A3+…
        fp2 = add(fp2, f64x(0x3EFA_01A0_1A01_D423)); // B4+…
        fp1 = mul(fp1, s);
        fp2 = mul(fp2, s);
        fp1 = add(fp1, pack(false, 0x3FF8, 0x8888_8888_8888_59AF)); // A2+…
        fp2 = add(fp2, pack(true, 0x3FF5, 0xB60B_60B6_0B61_D438)); // B3+…
        fp1 = mul(fp1, s);
        fp2 = mul(fp2, s);
        fp1 = add(fp1, pack(true, 0x3FFC, 0xAAAA_AAAA_AAAA_AA99)); // A1+…
        fp2 = add(fp2, pack(false, 0x3FFA, 0xAAAA_AAAA_AAAA_AB5E)); // B2+…
        fp1 = mul(fp1, s); // S(A1+…)
        let mut fp0 = mul(s, fp2); // S(B2+…)

        let rr = pack(r_sign, r_exp, r_sig);
        fp1 = mul(fp1, rr); // R'S(A1+…)
        fp0 = add(fp0, f32x(0xBF00_0000)); // B1+S(B2…)
        let ss = pack(s_sign, s_exp, s_sig);
        fp0 = mul(fp0, ss); // S'(B1+…)

        let cos = softfloat::floatx80_add(precision, mode, fp1, rr);
        let sin = softfloat::floatx80_add(precision, mode, fp0, f32x(posneg1));
        float_raise(flag::INEXACT);
        (sin, cos)
    } else {
        // NEVEN
        let j1 = n >> 1;
        let mut r_sign = sign(r);
        let r_exp = exp(r);
        let r_sig = frac(r);
        r_sign ^= j1 != 0;

        let s = mul(r, r); // S = R*R
        let mut fp1 = mul(f64x(0x3D2A_C4D0_D601_1EE3), s); // SB8
        let mut fp2 = mul(f64x(0xBD6A_AA77_CCC9_94F5), s); // SA7

        let mut s_sign = sign(s);
        let s_exp = exp(s);
        let s_sig = frac(s);
        s_sign ^= j1 != 0;
        let posneg1 = 0x3F80_0000u32 | if j1 != 0 { 0x8000_0000 } else { 0 };

        fp1 = add(fp1, f64x(0xBDA9_396F_9F45_AC19)); // B7+SB8
        fp2 = add(fp2, f64x(0x3DE6_1209_7AAE_8DA1)); // A6+SA7
        fp1 = mul(fp1, s);
        fp2 = mul(fp2, s);
        fp1 = add(fp1, f64x(0x3E21_EED9_0612_C972)); // B6+S(B7+SB8)
        fp2 = add(fp2, f64x(0xBE5A_E645_2A11_8AE4)); // A5+S(A6+SA7)
        fp1 = mul(fp1, s);
        fp2 = mul(fp2, s);
        fp1 = add(fp1, f64x(0xBE92_7E4F_B79D_9FCF)); // B5+…
        fp2 = add(fp2, f64x(0x3EC7_1DE3_A534_1531)); // A4+…
        fp1 = mul(fp1, s);
        fp2 = mul(fp2, s);
        fp1 = add(fp1, f64x(0x3EFA_01A0_1A01_D423)); // B4+…
        fp2 = add(fp2, f64x(0xBF2A_01A0_1A01_8B59)); // A3+…
        fp1 = mul(fp1, s);
        fp2 = mul(fp2, s);
        fp1 = add(fp1, pack(true, 0x3FF5, 0xB60B_60B6_0B61_D438)); // B3+…
        fp2 = add(fp2, pack(false, 0x3FF8, 0x8888_8888_8888_59AF)); // A2+…
        fp1 = mul(fp1, s);
        fp2 = mul(fp2, s);
        fp1 = add(fp1, pack(false, 0x3FFA, 0xAAAA_AAAA_AAAA_AB5E)); // B2+…
        fp2 = add(fp2, pack(true, 0x3FFC, 0xAAAA_AAAA_AAAA_AA99)); // A1+…
        fp1 = mul(fp1, s); // S(B2+…)
        let mut fp0 = mul(s, fp2); // S(A1+…)
        fp1 = add(fp1, f32x(0xBF00_0000)); // B1+S(B2…)

        let rr = pack(r_sign, r_exp, r_sig);
        fp0 = mul(fp0, rr); // R'S(A1+…)
        let ss = pack(s_sign, s_exp, s_sig);
        fp1 = mul(fp1, ss); // S'(B1+…)

        let cos = softfloat::floatx80_add(precision, mode, fp1, f32x(posneg1));
        let sin = softfloat::floatx80_add(precision, mode, fp0, rr);
        float_raise(flag::INEXACT);
        (sin, cos)
    }
}

// ─── Inverse-trigonometric / hyperbolic-arctan ──────────────────────────────

const PI_SIG: u64 = 0xC90F_DAA2_2168_C235;
const PIBY2_EXP: i32 = 0x3FFF;
const PI_EXP: i32 = 0x4000;

/// `floatx80_atan` (FATAN): arc tangent.
#[must_use]
pub fn floatx80_atan(precision: i32, mode: RoundingMode, a: FpReg) -> FpReg {
    let mut a_sig = frac(a);
    let a_exp = exp(a);
    let a_sign = sign(a);

    if a_exp == 0x7FFF {
        if sig_is_nan(a_sig) {
            return propagate_floatx80_nan_one_arg(a);
        }
        let pm = pack(a_sign, PIBY2_EXP, PI_SIG); // ±π/2
        float_raise(flag::INEXACT);
        return softfloat::floatx80_move(precision, mode, pm);
    }
    if a_exp == 0 && a_sig == 0 {
        return pack(a_sign, 0, 0);
    }

    let compact = make_compact(a_exp, a_sig);

    if !(0x3FFB_8000..=0x4002_FFFF).contains(&compact) {
        if compact > 0x3FFF_8000 {
            // |X| >= 16
            if compact > 0x4063_8000 {
                // |X| > 2^100: atan(X) ≈ ±π/2
                let fp0 = pack(a_sign, PIBY2_EXP, PI_SIG);
                let fp1 = pack(a_sign, 0x0001, ONE_SIG);
                let r = softfloat::floatx80_sub(precision, mode, fp0, fp1);
                float_raise(flag::INEXACT);
                r
            } else {
                let fp1d = div(pack(true, ONE_EXP, ONE_SIG), a); // X' = -1/X
                let xsave = fp1d;
                let fp0 = mul(fp1d, fp1d); // Y = X'*X'
                let z = mul(fp0, fp0); // Z = Y*Y
                let mut fp3 = mul(f64x(0xBFB7_0BF3_9853_9E6A), z); // Z*C5
                let mut fp2 = mul(f64x(0x3FBC_7187_962D_1D7D), z); // Z*C4
                fp3 = add(fp3, f64x(0xBFC2_4924_8271_07B8)); // C3+Z*C5
                fp2 = add(fp2, f64x(0x3FC9_9999_9996_263E)); // C2+Z*C4
                let fp1 = mul(z, fp3); // Z*(C3+Z*C5)
                let fp2 = mul(fp2, fp0); // Y*(C2+Z*C4)
                let fp1 = add(fp1, f64x(0xBFD5_5555_5555_5536)); // C1+Z*(C3+Z*C5)
                let fp0 = mul(fp0, xsave); // X'*Y
                let fp1 = add(fp1, fp2); // [Y*(C2+Z*C4)]+[C1+Z*(C3+Z*C5)]
                let fp0 = add(mul(fp0, fp1), xsave);
                let r =
                    softfloat::floatx80_add(precision, mode, fp0, pack(a_sign, PIBY2_EXP, PI_SIG));
                float_raise(flag::INEXACT);
                r
            }
        } else if compact < 0x3FD7_8000 {
            // |X| < 2^(-40): atan(X) ≈ X
            let r = softfloat::floatx80_move(precision, mode, a);
            float_raise(flag::INEXACT);
            r
        } else {
            // 2^(-40) <= |X| < 1/16
            let xsave = a;
            let fp0 = mul(a, a); // Y = X*X
            let z = mul(fp0, fp0); // Z = Y*Y
            let mut fp2 = mul(f64x(0x3FB3_4444_7F87_6989), z); // Z*B6
            let mut fp3 = mul(f64x(0xBFB7_44EE_7FAF_45DB), z); // Z*B5
            fp2 = add(fp2, f64x(0x3FBC_71C6_4694_0220)); // B4+Z*B6
            fp3 = add(fp3, f64x(0xBFC2_4924_9218_72F9)); // B3+Z*B5
            fp2 = mul(fp2, z); // Z*(B4+Z*B6)
            let fp1 = mul(z, fp3); // Z*(B3+Z*B5)
            fp2 = add(fp2, f64x(0x3FC9_9999_9999_8FA9)); // B2+Z*(B4+Z*B6)
            let fp1 = add(fp1, f64x(0xBFD5_5555_5555_5555)); // B1+Z*(B3+Z*B5)
            let fp2 = mul(fp2, fp0); // Y*(B2+Z*(B4+Z*B6))
            let fp0 = mul(fp0, xsave); // X*Y
            let fp1 = add(fp1, fp2);
            let fp0 = mul(fp0, fp1); // X*Y*(…)
            let r = softfloat::floatx80_add(precision, mode, fp0, xsave);
            float_raise(flag::INEXACT);
            r
        }
    } else {
        // 1/16 <= |X| < 16: table reduction
        a_sig &= 0xF800_0000_0000_0000;
        a_sig |= 0x0400_0000_0000_0000;
        let f = pack(a_sign, a_exp, a_sig); // F
        let fp1xf = mul(a, f); // X*F
        let fp0xmf = sub(a, f); // X-F
        let denom = add(fp1xf, pack(false, ONE_EXP, ONE_SIG)); // 1 + X*F
        let u = div(fp0xmf, denom); // U = (X-F)/(1+X*F)

        let mut tbl_index = compact;
        tbl_index &= 0x7FFF_0000;
        tbl_index -= 0x3FFB_0000;
        tbl_index >>= 1;
        tbl_index += compact & 0x0000_7800;
        tbl_index >>= 11;
        let mut fp3 = ATAN_TBL[tbl_index as usize];
        if a_sign {
            fp3.high |= 0x8000; // ATAN(F), signed
        }

        let v = mul(u, u); // V = U*U
        let mut fp2 = add(f64x(0xBFF6_687E_3149_87D8), v); // A3+V
        fp2 = mul(fp2, v); // V*(A3+V)
        let uv = mul(v, u); // U*V
        fp2 = add(fp2, f64x(0x4002_AC69_34A2_6DB3)); // A2+V*(A3+V)
        let uv = mul(uv, f64x(0xBFC2_476F_4E1D_A28E)); // A1*U*V
        let poly = mul(uv, fp2); // A1*U*V*(A2+V*(A3+V))
        let fp0 = add(u, poly); // ATAN(U)
        let r = softfloat::floatx80_add(precision, mode, fp0, fp3); // ATAN(X)
        float_raise(flag::INEXACT);
        r
    }
}

/// `floatx80_asin` (FASIN): arc sine.
#[must_use]
pub fn floatx80_asin(precision: i32, mode: RoundingMode, a: FpReg) -> FpReg {
    let a_sig = frac(a);
    let a_exp = exp(a);
    let a_sign = sign(a);

    if a_exp == 0x7FFF && sig_is_nan(a_sig) {
        return propagate_floatx80_nan_one_arg(a);
    }
    if a_exp == 0 && a_sig == 0 {
        return pack(a_sign, 0, 0);
    }

    let compact = make_compact(a_exp, a_sig);
    if compact >= 0x3FFF_8000 {
        // |X| >= 1
        if a_exp == ONE_EXP && a_sig == ONE_SIG {
            // |X| == 1: asin(±1) = ±π/2
            float_raise(flag::INEXACT);
            return softfloat::floatx80_move(precision, mode, pack(a_sign, PIBY2_EXP, PI_SIG));
        }
        float_raise(flag::INVALID); // |X| > 1
        return default_nan();
    }

    let one = pack(false, ONE_EXP, ONE_SIG);
    let fp1 = sub(one, a); // 1 - X
    let fp2 = add(one, a); // 1 + X
    let prod = mul(fp2, fp1); // (1+X)*(1-X)
    let root = softfloat::floatx80_sqrt(80, RN, prod); // SQRT((1+X)*(1-X))
    let fp0 = div(a, root); // X / SQRT(...)
    let r = floatx80_atan(precision, mode, fp0); // ATAN(...)
    float_raise(flag::INEXACT);
    r
}

/// `floatx80_acos` (FACOS): arc cosine.
#[must_use]
pub fn floatx80_acos(precision: i32, mode: RoundingMode, a: FpReg) -> FpReg {
    let a_sig = frac(a);
    let a_exp = exp(a);
    let a_sign = sign(a);

    if a_exp == 0x7FFF && sig_is_nan(a_sig) {
        return propagate_floatx80_nan_one_arg(a);
    }
    if a_exp == 0 && a_sig == 0 {
        // acos(0) = π/2
        float_raise(flag::INEXACT);
        return softfloat::round_and_pack_floatx80(precision, mode, false, PIBY2_EXP, PI_SIG, 0);
    }

    let compact = make_compact(a_exp, a_sig);
    if compact >= 0x3FFF_8000 {
        // |X| >= 1
        if a_exp == ONE_EXP && a_sig == ONE_SIG {
            if a_sign {
                // X == -1: acos(-1) = π
                float_raise(flag::INEXACT);
                return softfloat::floatx80_move(precision, mode, pack(false, PI_EXP, PI_SIG));
            }
            return pack(false, 0, 0); // acos(+1) = 0
        }
        float_raise(flag::INVALID); // |X| > 1
        return default_nan();
    }

    let one = pack(false, ONE_EXP, ONE_SIG);
    let fp1 = add(one, a); // 1 + X
    let fp0 = sub(one, a); // 1 - X
    let q = div(fp0, fp1); // (1-X)/(1+X)
    let root = softfloat::floatx80_sqrt(80, RN, q); // SQRT(...)
    let at = floatx80_atan(80, RN, root); // ATAN(SQRT(...))
    let r = softfloat::floatx80_add(precision, mode, at, at); // 2 * ATAN(...)
    float_raise(flag::INEXACT);
    r
}

/// `floatx80_atanh` (FATANH): hyperbolic arc tangent.
#[must_use]
pub fn floatx80_atanh(precision: i32, mode: RoundingMode, a: FpReg) -> FpReg {
    let a_sig = frac(a);
    let a_exp = exp(a);
    let a_sign = sign(a);

    if a_exp == 0x7FFF && sig_is_nan(a_sig) {
        return propagate_floatx80_nan_one_arg(a);
    }
    if a_exp == 0 && a_sig == 0 {
        return pack(a_sign, 0, 0);
    }

    let compact = make_compact(a_exp, a_sig);
    if compact >= 0x3FFF_8000 {
        // |X| >= 1
        if a_exp == ONE_EXP && a_sig == ONE_SIG {
            float_raise(flag::DIVBYZERO); // atanh(±1) = ±inf
            return pack(a_sign, 0x7FFF, 0);
        }
        float_raise(flag::INVALID); // |X| > 1
        return default_nan();
    }

    let one = pack(false, ONE_EXP, ONE_SIG);
    let half = pack(a_sign, 0x3FFE, ONE_SIG); // SIGN(X) * 1/2
    let y = pack(false, a_exp, a_sig); // |X|
    let neg_y = pack(true, a_exp, a_sig); // -|X|
    let two_y = add(y, y); // 2|X|
    let one_minus_y = add(neg_y, one); // 1 - |X|
    let z = div(two_y, one_minus_y); // Z = 2Y/(1-Y)
    let l = floatx80_lognp1(80, RN, z); // LOG1P(Z)
    let r = softfloat::floatx80_mul(precision, mode, l, half); // SIGN(X)*(1/2)*LOG1P(Z)
    float_raise(flag::INEXACT);
    r
}

// ─── Hyperbolic (sinh / cosh / tanh) ─────────────────────────────────────────

/// `floatx80_cosh` (FCOSH): hyperbolic cosine.
#[must_use]
pub fn floatx80_cosh(precision: i32, mode: RoundingMode, a: FpReg) -> FpReg {
    let a_sig = frac(a);
    let a_exp = exp(a);
    if a_exp == 0x7FFF {
        if sig_is_nan(a_sig) {
            return propagate_floatx80_nan_one_arg(a);
        }
        return pack(false, a_exp, a_sig); // cosh(±inf) = +inf
    }
    if a_exp == 0 && a_sig == 0 {
        return pack(false, ONE_EXP, ONE_SIG); // cosh(0) = 1
    }

    let compact = make_compact(a_exp, a_sig);
    if compact > 0x400C_B167 {
        if compact > 0x400C_B2B3 {
            let r = softfloat::round_and_pack_floatx80(precision, mode, false, 0x8000, ONE_SIG, 0);
            float_raise(flag::INEXACT);
            return r;
        }
        let mut fp0 = pack(false, a_exp, a_sig);
        fp0 = sub(fp0, f64x(0x40C6_2D38_D3D6_4634)); // |X| - 16381 log2 (lead)
        fp0 = sub(fp0, f64x(0x3D6F_90AE_B1E7_5CC7)); // … accurate
        fp0 = floatx80_etox(80, RN, fp0);
        let r = softfloat::floatx80_mul(precision, mode, fp0, pack(false, 0x7FFB, ONE_SIG));
        float_raise(flag::INEXACT);
        return r;
    }

    let mut fp0 = pack(false, a_exp, a_sig); // |X|
    fp0 = floatx80_etox(80, RN, fp0); // EXP(|X|)
    fp0 = mul(fp0, f32x(0x3F00_0000)); // (1/2)*EXP(|X|)
    let fp1 = div(f32x(0x3E80_0000), fp0); // 1/(2*EXP(|X|))
    let r = softfloat::floatx80_add(precision, mode, fp0, fp1);
    float_raise(flag::INEXACT);
    r
}

/// `floatx80_sinh` (FSINH): hyperbolic sine.
#[must_use]
pub fn floatx80_sinh(precision: i32, mode: RoundingMode, a: FpReg) -> FpReg {
    let a_sig = frac(a);
    let a_exp = exp(a);
    let a_sign = sign(a);
    if a_exp == 0x7FFF {
        if sig_is_nan(a_sig) {
            return propagate_floatx80_nan_one_arg(a);
        }
        return a; // sinh(±inf) = ±inf
    }
    if a_exp == 0 && a_sig == 0 {
        return pack(a_sign, 0, 0);
    }

    let compact = make_compact(a_exp, a_sig);
    if compact > 0x400C_B167 {
        if compact > 0x400C_B2B3 {
            let r = softfloat::round_and_pack_floatx80(precision, mode, a_sign, 0x8000, a_sig, 0);
            float_raise(flag::INEXACT);
            return r;
        }
        let mut fp0 = softfloat::floatx80_abs(80, RN, a); // |X|
        fp0 = sub(fp0, f64x(0x40C6_2D38_D3D6_4634));
        fp0 = sub(fp0, f64x(0x3D6F_90AE_B1E7_5CC7));
        fp0 = floatx80_etox(80, RN, fp0);
        let r = softfloat::floatx80_mul(precision, mode, fp0, pack(a_sign, 0x7FFB, ONE_SIG));
        float_raise(flag::INEXACT);
        return r;
    }

    let yabs = softfloat::floatx80_abs(80, RN, a); // Y = |X|
    let z = floatx80_etoxm1(80, RN, yabs); // Z = EXPM1(Y)
    let onepz = add(z, f32x(0x3F80_0000)); // 1+Z
    let fp0 = add(div(z, onepz), z); // Z/(1+Z) + Z
    let fact = 0x3F00_0000u32 | if a_sign { 0x8000_0000 } else { 0 }; // ±1/2
    let r = softfloat::floatx80_mul(precision, mode, fp0, f32x(fact));
    float_raise(flag::INEXACT);
    r
}

/// `floatx80_tanh` (FTANH): hyperbolic tangent.
#[must_use]
pub fn floatx80_tanh(precision: i32, mode: RoundingMode, a: FpReg) -> FpReg {
    let a_sig = frac(a);
    let a_exp = exp(a);
    let a_sign = sign(a);
    if a_exp == 0x7FFF {
        if sig_is_nan(a_sig) {
            return propagate_floatx80_nan_one_arg(a);
        }
        return pack(a_sign, ONE_EXP, ONE_SIG); // tanh(±inf) = ±1
    }
    if a_exp == 0 && a_sig == 0 {
        return pack(a_sign, 0, 0);
    }

    let compact = make_compact(a_exp, a_sig);
    if !(0x3FD7_8000..=0x3FFF_DDCE).contains(&compact) {
        if compact < 0x3FFF_8000 {
            // TANHSM: tanh(X) ≈ X
            let r = softfloat::floatx80_move(precision, mode, a);
            float_raise(flag::INEXACT);
            return r;
        } else if compact > 0x4004_8AA1 {
            // TANHHUGE: ±1 ∓ ε
            let s = 0x3F80_0000u32 | if a_sign { 0x8000_0000 } else { 0 };
            let eps = (s & 0x8000_0000) ^ 0x8080_0000;
            let r = softfloat::floatx80_add(precision, mode, f32x(s), f32x(eps));
            float_raise(flag::INEXACT);
            return r;
        } else {
            let y = pack(false, a_exp + 1, a_sig); // Y = 2|X|
            let mut fp0 = floatx80_etox(80, RN, y); // EXP(Y)
            fp0 = add(fp0, f32x(0x3F80_0000)); // EXP(Y)+1
            let sgn = if a_sign { 0x8000_0000u32 } else { 0 };
            let fp1 = div(f32x(sgn ^ 0xC000_0000), fp0); // -SIGN(X)*2 / [EXP(Y)+1]
            let r = softfloat::floatx80_add(precision, mode, fp1, f32x(sgn | 0x3F80_0000));
            float_raise(flag::INEXACT);
            return r;
        }
    }

    // 2^(-40) < |X| < (5/2)log2
    let y = pack(false, a_exp + 1, a_sig); // Y = 2|X|
    let z = floatx80_etoxm1(80, RN, y); // Z = EXPM1(Y)
    let zp2 = add(z, f32x(0x4000_0000)); // Z+2
    let den = pack(sign(zp2) ^ a_sign, exp(zp2), frac(zp2));
    let r = softfloat::floatx80_div(precision, mode, z, den);
    float_raise(flag::INEXACT);
    r
}
