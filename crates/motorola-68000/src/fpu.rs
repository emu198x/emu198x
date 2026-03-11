//! Motorola 68881/68882/68040 FPU support.
//!
//! Provides data type conversions, FPSR management, condition testing,
//! FMOVECR ROM constants, and arithmetic operations. Uses `f64` as
//! internal precision (covers Single/Double exactly, approximates
//! Extended's 64-bit mantissa with 53 bits).

use crate::registers::Registers;

// --- FP data formats ---

/// FPU source/destination data format, encoded in bits 12-10 of the
/// coprocessor extension word.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FpFormat {
    /// 32-bit integer (longword).
    Long = 0,
    /// 32-bit IEEE single precision.
    Single = 1,
    /// 96-bit Motorola extended precision (12 bytes).
    Extended = 2,
    /// 96-bit packed BCD (12 bytes).
    PackedBcd = 3,
    /// 16-bit integer (word).
    Word = 4,
    /// 64-bit IEEE double precision.
    Double = 5,
    /// 8-bit integer (byte).
    Byte = 6,
}

impl FpFormat {
    /// Decode format from 3-bit field (bits 12-10 of extension word).
    /// Returns None for the reserved encoding (7).
    pub fn from_bits(bits: u8) -> Option<Self> {
        match bits {
            0 => Some(Self::Long),
            1 => Some(Self::Single),
            2 => Some(Self::Extended),
            3 => Some(Self::PackedBcd),
            4 => Some(Self::Word),
            5 => Some(Self::Double),
            6 => Some(Self::Byte),
            _ => None,
        }
    }

    /// Number of bytes this format occupies in memory.
    #[must_use]
    pub const fn byte_size(self) -> usize {
        match self {
            Self::Byte => 1,
            Self::Word => 2,
            Self::Long | Self::Single => 4,
            Self::Double => 8,
            Self::Extended | Self::PackedBcd => 12,
        }
    }
}

// --- Data type conversions ---

/// Convert raw memory bytes to f64 based on FP format.
/// `data` must contain at least `format.byte_size()` bytes.
pub fn bytes_to_f64(data: &[u8], format: FpFormat) -> f64 {
    match format {
        FpFormat::Byte => {
            // Sign-extended byte to integer, then to f64
            let v = data[0] as i8;
            f64::from(v)
        }
        FpFormat::Word => {
            let v = i16::from_be_bytes([data[0], data[1]]);
            f64::from(v)
        }
        FpFormat::Long => {
            let v = i32::from_be_bytes([data[0], data[1], data[2], data[3]]);
            f64::from(v)
        }
        FpFormat::Single => {
            let bits = u32::from_be_bytes([data[0], data[1], data[2], data[3]]);
            f64::from(f32::from_bits(bits))
        }
        FpFormat::Double => {
            let bits = u64::from_be_bytes([
                data[0], data[1], data[2], data[3], data[4], data[5], data[6], data[7],
            ]);
            f64::from_bits(bits)
        }
        FpFormat::Extended => {
            decode_extended(data)
        }
        FpFormat::PackedBcd => {
            decode_packed_bcd(data)
        }
    }
}

/// Convert f64 to raw memory bytes for the given format.
/// Returns a Vec with exactly `format.byte_size()` bytes.
pub fn f64_to_bytes(val: f64, format: FpFormat) -> Vec<u8> {
    match format {
        FpFormat::Byte => {
            let v = val as i8;
            vec![v as u8]
        }
        FpFormat::Word => {
            let v = val as i16;
            v.to_be_bytes().to_vec()
        }
        FpFormat::Long => {
            let v = val as i32;
            v.to_be_bytes().to_vec()
        }
        FpFormat::Single => {
            let v = val as f32;
            v.to_bits().to_be_bytes().to_vec()
        }
        FpFormat::Double => {
            val.to_bits().to_be_bytes().to_vec()
        }
        FpFormat::Extended => {
            encode_extended(val)
        }
        FpFormat::PackedBcd => {
            encode_packed_bcd(val)
        }
    }
}

/// Decode Motorola 96-bit extended-precision format.
///
/// Memory layout (12 bytes, big-endian):
///   [0]:    sign (bit 7) + exponent high 7 bits
///   [1]:    exponent low 8 bits
///   [2-3]:  padding (zero)
///   [4-11]: 64-bit mantissa (with explicit integer bit)
fn decode_extended(data: &[u8]) -> f64 {
    let sign = (data[0] >> 7) & 1;
    let exponent = (u16::from(data[0] & 0x7F) << 8) | u16::from(data[1]);
    let mantissa = u64::from_be_bytes([
        data[4], data[5], data[6], data[7], data[8], data[9], data[10], data[11],
    ]);

    if exponent == 0 && mantissa == 0 {
        return if sign != 0 { -0.0 } else { 0.0 };
    }

    if exponent == 0x7FFF {
        if mantissa == 0 {
            return if sign != 0 {
                f64::NEG_INFINITY
            } else {
                f64::INFINITY
            };
        }
        return f64::NAN;
    }

    // Extended uses bias 16383. The mantissa has an explicit integer bit (bit 63).
    // Convert to f64: value = (-1)^sign * 2^(exponent - 16383) * (mantissa / 2^63)
    let frac = mantissa as f64 / (1u64 << 63) as f64;
    let exp = (exponent as i32) - 16383;
    let magnitude = frac * f64::from(2.0f32).powi(exp);
    if sign != 0 {
        -magnitude
    } else {
        magnitude
    }
}

/// Encode f64 to Motorola 96-bit extended-precision format (12 bytes).
fn encode_extended(val: f64) -> Vec<u8> {
    let mut result = vec![0u8; 12];

    if val == 0.0 {
        if val.is_sign_negative() {
            result[0] = 0x80;
        }
        return result;
    }

    if val.is_nan() {
        result[0] = 0x7F;
        result[1] = 0xFF;
        result[4] = 0x7F; // quiet NaN
        result[5] = 0xFF;
        result[6] = 0xFF;
        result[7] = 0xFF;
        result[8] = 0xFF;
        result[9] = 0xFF;
        result[10] = 0xFF;
        result[11] = 0xFF;
        return result;
    }

    if val.is_infinite() {
        result[0] = if val.is_sign_negative() { 0xFF } else { 0x7F };
        result[1] = 0xFF;
        return result;
    }

    let sign = val.is_sign_negative();
    let abs_val = val.abs();

    // Decompose: abs_val = frac * 2^exp where 1.0 <= frac < 2.0
    // For extended, mantissa has explicit integer bit.
    let (frac, exp) = frexp(abs_val);
    // frexp returns 0.5 <= frac < 1.0 and exp such that val = frac * 2^exp
    // We want 1.0 <= m < 2.0 so m = frac * 2, exponent = exp - 1
    let m = frac * 2.0;
    let biased_exp = (exp - 1 + 16383) as u16;

    // Convert mantissa to 64-bit integer (m * 2^63)
    let mantissa_bits = (m * (1u64 << 63) as f64) as u64;

    result[0] = (if sign { 0x80 } else { 0 }) | ((biased_exp >> 8) as u8 & 0x7F);
    result[1] = biased_exp as u8;
    // bytes 2-3 are padding (zero)
    let mant_bytes = mantissa_bits.to_be_bytes();
    result[4..12].copy_from_slice(&mant_bytes);

    result
}

/// Decode 96-bit packed BCD format to f64.
///
/// Layout: sign (bit 95), sign of exponent (bit 94), 3-digit BCD exponent,
/// then 17-digit BCD mantissa with implicit decimal point after first digit.
fn decode_packed_bcd(data: &[u8]) -> f64 {
    let sign = (data[0] >> 7) & 1;
    let exp_sign = (data[0] >> 6) & 1;

    // Exponent: 3 BCD digits in bits 3-0 of bytes 0, 1 high nybble, 1 low nybble
    let exp_d2 = data[0] & 0x0F;
    let exp_d1 = (data[1] >> 4) & 0x0F;
    let exp_d0 = data[1] & 0x0F;
    let exp_val = u32::from(exp_d2) * 100 + u32::from(exp_d1) * 10 + u32::from(exp_d0);
    let exponent = if exp_sign != 0 {
        -(exp_val as i32)
    } else {
        exp_val as i32
    };

    // Mantissa: 17 BCD digits starting at byte 2 high nybble
    // First digit is the integer part, remaining 16 are fractional
    let mut mantissa = 0.0f64;
    let mut digit_idx = 0;
    for &byte in &data[2..12] {
        let hi = (byte >> 4) & 0x0F;
        let lo = byte & 0x0F;
        for &d in &[hi, lo] {
            if digit_idx < 17 {
                mantissa = mantissa * 10.0 + f64::from(d);
            }
            digit_idx += 1;
        }
    }
    // Mantissa has implicit decimal point after first digit:
    // actual_value = mantissa / 10^16 * 10^exponent
    let result = mantissa * 10.0f64.powi(exponent - 16);

    if sign != 0 {
        -result
    } else {
        result
    }
}

/// Encode f64 to 96-bit packed BCD format (12 bytes).
fn encode_packed_bcd(val: f64) -> Vec<u8> {
    let mut result = vec![0u8; 12];

    if val == 0.0 || val.is_nan() || val.is_infinite() {
        if val.is_sign_negative() || val.is_nan() {
            result[0] = 0x80;
        }
        if val.is_nan() {
            result[0] |= 0x7F;
            result[1] = 0xFF;
        }
        if val.is_infinite() {
            result[0] |= 0x7F;
            result[1] = 0xFF;
        }
        return result;
    }

    let sign = val < 0.0;
    let abs_val = val.abs();

    // Find the base-10 exponent
    let log10 = abs_val.log10().floor() as i32;
    // Scale so we have 17 significant digits
    let scaled = abs_val / 10.0f64.powi(log10 - 16);
    let mut digits = scaled.round() as u64;

    // Encode mantissa: 17 BCD digits into bytes 2-11 (20 nybbles, last 3 unused)
    let mut bcd_digits = [0u8; 20];
    for i in (0..17).rev() {
        bcd_digits[i] = (digits % 10) as u8;
        digits /= 10;
    }
    for (i, byte) in result[2..12].iter_mut().enumerate() {
        let di = i * 2;
        *byte = (bcd_digits[di] << 4) | bcd_digits[di + 1];
    }

    // Encode exponent
    let exp_sign = log10 < 0;
    let exp_abs = log10.unsigned_abs();
    let exp_d2 = ((exp_abs / 100) % 10) as u8;
    let exp_d1 = ((exp_abs / 10) % 10) as u8;
    let exp_d0 = (exp_abs % 10) as u8;

    result[0] = (if sign { 0x80 } else { 0 }) | (if exp_sign { 0x40 } else { 0 }) | exp_d2;
    result[1] = (exp_d1 << 4) | exp_d0;

    result
}

/// Split f64 into fraction and exponent (like C frexp).
/// Returns (frac, exp) where val = frac * 2^exp, 0.5 <= |frac| < 1.0.
fn frexp(val: f64) -> (f64, i32) {
    if val == 0.0 || val.is_nan() || val.is_infinite() {
        return (val, 0);
    }
    let bits = val.to_bits();
    let biased_exp = ((bits >> 52) & 0x7FF) as i32;
    let exp = biased_exp - 1022; // -1023 + 1 to get 0.5 <= frac < 1.0
    // Rebuild with exponent = 1022 (biased), giving 0.5 <= |frac| < 1.0
    let frac_bits = (bits & 0x800F_FFFF_FFFF_FFFF) | (1022u64 << 52);
    (f64::from_bits(frac_bits), exp)
}

// --- FPSR condition code management ---

/// Set FPSR condition codes based on an f64 result value.
pub fn set_fpcc(regs: &mut Registers, val: f64) {
    let n = val.is_sign_negative() && !val.is_nan() && val != 0.0;
    let z = val == 0.0;
    let i = val.is_infinite();
    let nan = val.is_nan();
    regs.set_fpsr_cc(n, z, i, nan);
}

// --- FPU condition testing ---

/// Test an FPU condition code against the current FPSR.
/// `condition` is the 6-bit condition field from FBcc/FScc/FDBcc/FTRAPcc.
/// Returns true if the condition is met.
///
/// Condition code bits in FPSR (bits 27-24): N=3, Z=2, I=1, NAN=0
pub fn test_condition(fpsr: u32, condition: u8) -> bool {
    let cc = (fpsr >> 24) & 0x0F;
    let nan = cc & 1 != 0;
    let i = cc & 2 != 0;
    let z = cc & 4 != 0;
    let n = cc & 8 != 0;

    // The 32 condition codes follow the 68881/68882 User's Manual Table 3-7.
    // Bit 4 of the condition inverts IEEE-awareness (BSUN signalling).
    let pred = condition & 0x0F;
    let result = match pred {
        // False / True
        0x00 => false,           // F / SF
        0x01 => z,               // EQ / SEQ
        0x02 => !(nan | z | n),  // OGT / GT
        0x03 => z | !(nan | n),  // OGE / GE
        0x04 => n & !(nan | z),  // OLT / LT
        0x05 => z | (n & !nan),  // OLE / LE
        0x06 => !(nan | z),      // OGL / GL
        0x07 => !nan,            // OR / GLE
        0x08 => nan,             // UN / NGLE
        0x09 => nan | z,         // UEQ / NGL
        0x0A => nan | !(n | z),  // UGT / NLE
        0x0B => nan | z | !n,    // UGE / NLT
        0x0C => nan | (n & !z),  // ULT / NGE
        0x0D => nan | z | n,     // ULE / NGT
        0x0E => !z,              // NE / SNE
        0x0F => true,            // T / ST
        _ => unreachable!(),
    };

    // Conditions 0x10-0x1F are the same predicates but with BSUN signalling
    // on NaN. We don't model BSUN exceptions, so the result is the same.
    // The bit-4 flag only affects whether BSUN is raised, not the boolean result.
    let _ = i; // suppress unused warning; I flag used for specific conditions above indirectly

    result
}

// --- FMOVECR ROM constants ---

/// Return the FPU ROM constant for the given offset (0-127).
/// Only offsets $00-$0B and $30-$3F are defined; others return 0.0.
pub fn fmovecr_constant(offset: u8) -> f64 {
    match offset {
        0x00 => core::f64::consts::PI,
        0x0B => core::f64::consts::LOG10_2,
        0x0C => core::f64::consts::E,
        0x0D => core::f64::consts::LOG2_E,
        0x0E => core::f64::consts::LOG10_E,
        0x0F => 0.0,
        0x30 => core::f64::consts::LN_2,
        0x31 => core::f64::consts::LN_10,
        0x32 => 1e0,
        0x33 => 1e1,
        0x34 => 1e2,
        0x35 => 1e4,
        0x36 => 1e8,
        0x37 => 1e16,
        0x38 => 1e32,
        0x39 => 1e64,
        0x3A => 1e128,
        0x3B => 1e256,
        0x3C => f64::INFINITY, // 1e512 overflows f64 (fits in extended)
        0x3D => f64::INFINITY, // 1e1024
        0x3E => f64::INFINITY, // 1e2048
        0x3F => f64::INFINITY, // 1e4096
        _ => 0.0,
    }
}

// --- Rounding ---

/// Apply FPCR rounding mode to a result value.
pub fn apply_rounding(val: f64, rounding_mode: u8, rounding_precision: u8) -> f64 {
    // First apply precision rounding
    let val = match rounding_precision {
        1 => {
            // Single precision: round to f32 and back
            (val as f32) as f64
        }
        2 => {
            // Double precision: already f64, no change
            val
        }
        _ => {
            // Extended (0) or reserved (3): f64 is our best approximation
            val
        }
    };

    // Then apply rounding mode
    match rounding_mode {
        0 => val, // Round to nearest (default, f64 already does this)
        1 => {
            // Round toward zero (truncate)
            val.trunc()
        }
        2 => {
            // Round toward minus infinity (floor)
            val.floor()
        }
        3 => {
            // Round toward plus infinity (ceil)
            val.ceil()
        }
        _ => val,
    }
}

// --- Arithmetic operations ---
//
// Each function takes source (and optionally destination) f64 values,
// performs the operation, and returns the result. FPSR updates are done
// by the caller using set_fpcc().

/// FINT: round to integer using the given rounding mode.
pub fn fint(val: f64, rounding_mode: u8) -> f64 {
    match rounding_mode {
        0 => val.round_ties_even(),
        1 => val.trunc(),
        2 => val.floor(),
        3 => val.ceil(),
        _ => val.round_ties_even(),
    }
}

/// FINTRZ: round to integer toward zero.
pub fn fintrz(val: f64) -> f64 {
    val.trunc()
}

/// FSCALE: multiply val by 2^scale.
pub fn fscale(val: f64, scale: f64) -> f64 {
    // scale should be an integer value
    let n = scale.trunc() as i32;
    val * 2.0f64.powi(n)
}

/// FGETEXP: extract unbiased exponent as integer.
pub fn fgetexp(val: f64) -> f64 {
    if val == 0.0 || val.is_nan() || val.is_infinite() {
        if val == 0.0 {
            return 0.0;
        }
        return val; // NaN/Inf pass through
    }
    let (_, exp) = frexp(val);
    f64::from(exp - 1) // frexp returns exp such that 0.5 <= frac < 1.0
}

/// FGETMAN: extract mantissa (1.0 <= |result| < 2.0).
pub fn fgetman(val: f64) -> f64 {
    if val == 0.0 || val.is_nan() || val.is_infinite() {
        return val;
    }
    let (frac, _) = frexp(val);
    frac * 2.0 // frexp gives 0.5 <= frac < 1.0, we want 1.0 <= man < 2.0
}

/// FMOD: IEEE modulo (sign of dividend).
pub fn fmod(dividend: f64, divisor: f64) -> f64 {
    dividend % divisor
}

/// FREM: IEEE remainder (round quotient to nearest).
pub fn frem(dividend: f64, divisor: f64) -> f64 {
    // IEEE remainder: dividend - round(dividend/divisor) * divisor
    let q = (dividend / divisor).round_ties_even();
    dividend - q * divisor
}

/// FSINCOS: compute both sin and cos simultaneously.
/// Returns (sin, cos).
pub fn fsincos(val: f64) -> (f64, f64) {
    (val.sin(), val.cos())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn bytes_to_f64_single() {
        // 1.0f32 = 0x3F800000
        let data = [0x3F, 0x80, 0x00, 0x00];
        let val = bytes_to_f64(&data, FpFormat::Single);
        assert!((val - 1.0).abs() < f64::EPSILON);
    }

    #[test]
    fn bytes_to_f64_double() {
        // 1.0f64 = 0x3FF0000000000000
        let data = [0x3F, 0xF0, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00];
        let val = bytes_to_f64(&data, FpFormat::Double);
        assert!((val - 1.0).abs() < f64::EPSILON);
    }

    #[test]
    fn bytes_to_f64_long() {
        // -42 as i32
        let data = (-42i32).to_be_bytes();
        let val = bytes_to_f64(&data, FpFormat::Long);
        assert!((val - (-42.0)).abs() < f64::EPSILON);
    }

    #[test]
    fn bytes_to_f64_word() {
        let data = 1000i16.to_be_bytes();
        let val = bytes_to_f64(&data, FpFormat::Word);
        assert!((val - 1000.0).abs() < f64::EPSILON);
    }

    #[test]
    fn bytes_to_f64_byte() {
        let val = bytes_to_f64(&[0xFE], FpFormat::Byte);
        assert!((val - (-2.0)).abs() < f64::EPSILON);
    }

    #[test]
    fn extended_roundtrip() {
        let values = [1.0, -1.0, 0.0, core::f64::consts::PI, 1e10, -1e-10];
        for &v in &values {
            let encoded = encode_extended(v);
            assert_eq!(encoded.len(), 12);
            let decoded = decode_extended(&encoded);
            if v == 0.0 {
                assert!(decoded == 0.0);
            } else {
                let rel_err = ((decoded - v) / v).abs();
                assert!(rel_err < 1e-15, "roundtrip failed for {v}: got {decoded}, rel_err {rel_err}");
            }
        }
    }

    #[test]
    fn extended_infinity() {
        let pos_inf = encode_extended(f64::INFINITY);
        assert_eq!(decode_extended(&pos_inf), f64::INFINITY);
        let neg_inf = encode_extended(f64::NEG_INFINITY);
        assert_eq!(decode_extended(&neg_inf), f64::NEG_INFINITY);
    }

    #[test]
    fn extended_nan() {
        let nan = encode_extended(f64::NAN);
        assert!(decode_extended(&nan).is_nan());
    }

    #[test]
    fn f64_to_single_bytes() {
        let bytes = f64_to_bytes(1.0, FpFormat::Single);
        assert_eq!(bytes, vec![0x3F, 0x80, 0x00, 0x00]);
    }

    #[test]
    fn fmovecr_pi() {
        let pi = fmovecr_constant(0x00);
        assert!((pi - core::f64::consts::PI).abs() < 1e-15);
    }

    #[test]
    fn fmovecr_powers_of_ten() {
        assert!((fmovecr_constant(0x32) - 1.0).abs() < f64::EPSILON);
        assert!((fmovecr_constant(0x33) - 10.0).abs() < f64::EPSILON);
        assert!((fmovecr_constant(0x34) - 100.0).abs() < f64::EPSILON);
    }

    #[test]
    fn condition_eq() {
        // Z=1 → EQ is true
        let fpsr = 0x0400_0000; // Z bit set
        assert!(test_condition(fpsr, 0x01));
        // Z=0 → EQ is false
        assert!(!test_condition(0, 0x01));
    }

    #[test]
    fn condition_ne() {
        assert!(test_condition(0, 0x0E)); // Z=0 → NE true
        assert!(!test_condition(0x0400_0000, 0x0E)); // Z=1 → NE false
    }

    #[test]
    fn condition_gt() {
        // OGT (pred 0x02): !(NAN | Z | N)
        // No flags set → GT is true (positive, non-zero, not NaN)
        assert!(test_condition(0, 0x02));
        // N set → GT is false
        assert!(!test_condition(0x0800_0000, 0x02));
    }

    #[test]
    fn set_fpcc_positive() {
        let mut regs = Registers::new();
        set_fpcc(&mut regs, 42.0);
        assert_eq!(regs.fpsr_condition_code() & 0x0F, 0); // no flags
    }

    #[test]
    fn set_fpcc_negative() {
        let mut regs = Registers::new();
        set_fpcc(&mut regs, -1.0);
        assert_eq!(regs.fpsr_condition_code() & 8, 8); // N flag
    }

    #[test]
    fn set_fpcc_zero() {
        let mut regs = Registers::new();
        set_fpcc(&mut regs, 0.0);
        assert_eq!(regs.fpsr_condition_code() & 4, 4); // Z flag
    }

    #[test]
    fn set_fpcc_nan() {
        let mut regs = Registers::new();
        set_fpcc(&mut regs, f64::NAN);
        assert_eq!(regs.fpsr_condition_code() & 1, 1); // NAN flag
    }

    #[test]
    fn set_fpcc_infinity() {
        let mut regs = Registers::new();
        set_fpcc(&mut regs, f64::INFINITY);
        assert_eq!(regs.fpsr_condition_code() & 2, 2); // I flag
    }

    #[test]
    fn fint_round_to_nearest() {
        assert!((fint(2.5, 0) - 2.0).abs() < f64::EPSILON); // round ties to even
        assert!((fint(3.5, 0) - 4.0).abs() < f64::EPSILON);
        assert!((fint(2.7, 0) - 3.0).abs() < f64::EPSILON);
    }

    #[test]
    fn fint_toward_zero() {
        assert!((fint(2.9, 1) - 2.0).abs() < f64::EPSILON);
        assert!((fint(-2.9, 1) - (-2.0)).abs() < f64::EPSILON);
    }

    #[test]
    fn fscale_doubles() {
        assert!((fscale(1.0, 3.0) - 8.0).abs() < f64::EPSILON); // 1.0 * 2^3 = 8
    }

    #[test]
    fn fgetexp_basic() {
        // 8.0 = 1.0 * 2^3, so exponent = 3
        assert!((fgetexp(8.0) - 3.0).abs() < f64::EPSILON);
    }

    #[test]
    fn fgetman_basic() {
        // 8.0 = 1.0 * 2^3, so mantissa = 1.0
        assert!((fgetman(8.0) - 1.0).abs() < f64::EPSILON);
        // 12.0 = 1.5 * 2^3, so mantissa = 1.5
        assert!((fgetman(12.0) - 1.5).abs() < f64::EPSILON);
    }

    #[test]
    fn fsincos_basic() {
        let (s, c) = fsincos(0.0);
        assert!((s - 0.0).abs() < f64::EPSILON);
        assert!((c - 1.0).abs() < f64::EPSILON);
    }
}
