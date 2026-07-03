//! SID D/A converter model (issue #64).
//!
//! Ported from reSID's `dac.cc` (Dag Lem). The SID's waveform (12-bit) and
//! envelope (8-bit) outputs pass through R-2R ladder DACs that are *not* ideal:
//!
//! - The **6581** DACs omit the bit-0 termination resistor and have a poor
//!   `2R/R` match (`≈ 2.20`), so the lower bits are badly weighted — the output
//!   for bit 0 equals the output for bit 1 — and higher bits step unevenly.
//!   This nonlinearity is a big part of the 6581's gritty character.
//! - The **8580** DACs are near-ideal (`2R/R = 2.00`, correct termination), so
//!   their transfer is essentially linear.
//!
//! Even a nominally-off transistor leaks a little (subthreshold conduction), so
//! an unset bit still contributes a small `leakage` term.
//!
//! The tables are built once per model and shared (like reSID's static
//! `model_dac`), so nothing DAC-related lands in the serialized SID state.

use std::sync::OnceLock;

use crate::SidModel;

/// Subthreshold leakage of a nominally-off ladder transistor (reSID).
const MOSFET_LEAKAGE_6581: f64 = 0.0075;
const MOSFET_LEAKAGE_8580: f64 = 0.0035;

/// Waveform-DAC "zero" level, in 12-bit DAC-output units. On the 6581 the DAC
/// zero sits well below mid-scale (`$800`), measured at `$380` — this is the DC
/// offset the envelope multiplies and the output stage AC-couples away. The
/// 8580 has effectively no offset. (reSID `voice.cc` `wave_zero`.)
#[must_use]
pub fn wave_zero(model: SidModel) -> f32 {
    match model {
        SidModel::Mos6581 => f32::from(0x380u16),
        SidModel::Mos8580 => f32::from(0x9E0u16),
    }
}

/// The 12-bit waveform DAC transfer for `model`, indexed by the raw waveform
/// value (`0..=4095`), output scaled to `0.0..=4095.0`.
#[must_use]
pub fn wave_dac(model: SidModel) -> &'static [f32] {
    static TABLES: OnceLock<[Vec<f32>; 2]> = OnceLock::new();
    let tables = TABLES.get_or_init(|| {
        [
            build_dac_table(12, 2.20, false), // 6581: missing termination
            build_dac_table(12, 2.00, true),  // 8580: ideal ladder
        ]
    });
    &tables[model.index()]
}

/// The 8-bit envelope DAC transfer for `model`, indexed by the envelope level
/// (`0..=255`), output scaled to `0.0..=255.0`.
#[must_use]
pub fn env_dac(model: SidModel) -> &'static [f32] {
    static TABLES: OnceLock<[Vec<f32>; 2]> = OnceLock::new();
    let tables = TABLES.get_or_init(|| {
        [
            build_dac_table(8, 2.20, false),
            build_dac_table(8, 2.00, true),
        ]
    });
    &tables[model.index()]
}

/// Build one DAC lookup table by superpositioning per-bit voltages in an R-2R
/// ladder. Direct port of reSID `build_dac_table`; `term` selects the 8580's
/// terminated ladder (else the 6581's un-terminated one). Output is scaled so
/// the all-bits-set input maps to `2^bits - 1`.
///
/// Kept at `f64` precision because the filter's cutoff DAC
/// ([`crate::filter_tables`]) maps this through further `f64` scaling before
/// rounding, exactly as reSID does.
pub(crate) fn build_dac_table_f64(bits: usize, r2_div_r: f64, term: bool) -> Vec<f64> {
    let leakage = if term {
        MOSFET_LEAKAGE_8580
    } else {
        MOSFET_LEAKAGE_6581
    };

    // Voltage contribution of each individual set bit.
    let mut vbit = vec![0.0f64; bits];
    for (set_bit, slot) in vbit.iter_mut().enumerate() {
        let mut vn = 1.0f64; // normalized bit voltage
        let r = 1.0f64;
        let two_r = r2_div_r * r;
        // Rn = 2R for correct termination, infinite for the missing one.
        let mut rn = if term { two_r } else { f64::INFINITY };

        // DAC "tail" resistance by repeated parallel substitution.
        for _ in 0..set_bit {
            rn = if rn.is_infinite() {
                r + two_r
            } else {
                r + two_r * rn / (two_r + rn) // R + 2R || Rn
            };
        }

        // Source transformation for the bit voltage.
        if rn.is_infinite() {
            rn = two_r;
        } else {
            rn = two_r * rn / (two_r + rn); // 2R || Rn
            vn = vn * rn / two_r;
        }

        // Output voltage by repeated source transformation from the tail.
        for _ in (set_bit + 1)..bits {
            rn += r;
            let i = vn / rn;
            rn = two_r * rn / (two_r + rn); // 2R || Rn
            vn = rn * i;
        }

        *slot = vn;
    }

    // Superposition: sum the contributing bits (leaking the off ones) for every
    // input combination, scaled so all-bits-set -> 2^bits - 1.
    let full_scale = ((1usize << bits) - 1) as f64;
    (0..(1usize << bits))
        .map(|i| {
            let mut vo = 0.0f64;
            for (j, &v) in vbit.iter().enumerate() {
                let bit_set = (i >> j) & 1 == 1;
                vo += if bit_set { 1.0 } else { leakage } * v;
            }
            full_scale * vo
        })
        .collect()
}

fn build_dac_table(bits: usize, r2_div_r: f64, term: bool) -> Vec<f32> {
    build_dac_table_f64(bits, r2_div_r, term)
        .into_iter()
        .map(|v| v as f32)
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn tables_have_the_right_length_and_endpoints() {
        for model in [SidModel::Mos6581, SidModel::Mos8580] {
            let wave = wave_dac(model);
            let env = env_dac(model);
            assert_eq!(wave.len(), 4096);
            assert_eq!(env.len(), 256);
            // Zero input -> ~zero output (only leakage), full input -> full scale.
            assert!(wave[0] < 40.0, "wave[0] should be near zero");
            assert!((wave[4095] - 4095.0).abs() < 1.0, "wave[max] = full scale");
            assert!((env[255] - 255.0).abs() < 1.0, "env[max] = full scale");
        }
    }

    #[test]
    fn six581_has_the_missing_bit0_termination_discontinuity() {
        // reSID's headline 6581 defect: with no bit-0 termination resistor, the
        // output for bit 0 equals the output for bit 1. So dac[1] ≈ dac[2].
        let wave = wave_dac(SidModel::Mos6581);
        let bit0 = wave[1]; // only bit 0 set
        let bit1 = wave[2]; // only bit 1 set
        assert!(
            (bit0 - bit1).abs() < 1.0,
            "6581 bit0 ({bit0}) should read like bit1 ({bit1})"
        );
    }

    #[test]
    fn eight580_ladder_is_near_linear() {
        // The 8580's terminated, well-matched ladder has no bit-0 collapse: each
        // higher bit is close to twice the previous one. Every single-bit output
        // shares a common leakage floor (`dac[0]`), so subtract it to recover the
        // bit weights before comparing.
        let wave = wave_dac(SidModel::Mos8580);
        let floor = wave[0];
        let bit0 = wave[1] - floor;
        let bit1 = wave[2] - floor;
        assert!(
            bit0 > 0.5,
            "8580 bit0 weight ({bit0}) should be a real step"
        );
        assert!(
            (bit1 - 2.0 * bit0).abs() < 0.2,
            "8580 bit1 weight ({bit1}) should be ~2x bit0 ({bit0})"
        );
    }

    #[test]
    fn tables_are_monotonic_nondecreasing_for_the_8580() {
        // The linear ladder never steps backwards as the code counts up.
        let wave = wave_dac(SidModel::Mos8580);
        assert!(wave.windows(2).all(|w| w[1] >= w[0] - 1.0));
    }
}
