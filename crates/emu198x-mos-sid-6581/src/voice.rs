//! SID voice: oscillator, waveform generation, ring mod, sync, and noise.

#![allow(clippy::cast_possible_truncation)]

use serde::{Deserialize, Serialize};

use crate::SidModel;

use crate::combined_wave_tables::{
    COMBINED_P_T_6581, COMBINED_P_T_8580, COMBINED_PS_6581, COMBINED_PS_8580, COMBINED_PST_6581,
    COMBINED_PST_8580, COMBINED_TRI_SAW_6581, COMBINED_TRI_SAW_8580,
};

const NOISE_LFSR_SEED: u32 = 0x7F_FFFF;
const NOISE_LFSR_MASK: u32 = 0x7F_FFFF;

const CONTROL_TEST: u8 = 0x08;

/// Noise shift-register bit and the waveform DAC input it drives, for each
/// of the eight noise outputs (see [`Voice::noise_output`]).
const NOISE_TAPS: [(u32, u32); 8] = [
    (20, 11),
    (18, 10),
    (14, 9),
    (11, 8),
    (9, 7),
    (5, 6),
    (2, 5),
    (0, 4),
];

/// Cycles TEST must be held before the noise shift register's SRAM cells
/// start reaching one, then the cycles between each further bit. While TEST
/// is set the register bits are interconnected and the cells drift up towards
/// one; after the full ramp the register reads all ones (`0x7FFFFF`). Values
/// from reSID `wave.cc` (`SHIFT_REGISTER_RESET_*`). reSIDfp measures the
/// same effect on warm chips with different figures (6581R3 50 000/15 000,
/// 8580R5 986 000/314 300) and notes the times vary with temperature and chip;
/// VICE's `testprogs/SID/bitfade` `delaynoise` reads about $8000 on reSID's
/// 6581 and $950000 on a real 8580R5. The 8580 holds its bits far longer.
const SHIFT_REGISTER_RESET_START_6581: u32 = 35_000;
const SHIFT_REGISTER_RESET_BIT_6581: u32 = 1_000;
const SHIFT_REGISTER_RESET_START_8580: u32 = 2_519_864;
const SHIFT_REGISTER_RESET_BIT_8580: u32 = 315_000;

/// Cycles a floating waveform DAC input holds its last value after the
/// waveform is deselected, then the cycles between each further fade step.
/// Values from reSID `wave.cc` (`FLOATING_OUTPUT_TTL_*`: about 200 ms on the
/// 6581, 5 s on the 8580; reSID notes two samplings showing the DAC keeps its
/// state for at least $14000 cycles). reSIDfp uses 54 000/1 400 (6581R3) and
/// 800 000/50 000 (8580R5). VICE's `testprogs/SID/osc3-wave0` expects OSC3 to
/// hold and then fade to zero within about 3 s.
const FLOATING_OUTPUT_TTL_START_6581: u32 = 182_000;
const FLOATING_OUTPUT_TTL_BIT_6581: u32 = 1_500;
const FLOATING_OUTPUT_TTL_START_8580: u32 = 4_400_000;
const FLOATING_OUTPUT_TTL_BIT_8580: u32 = 50_000;

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Voice {
    pub accumulator: u32,
    pub frequency: u16,
    pub pulse_width: u16,
    pub control: u8,
    pub noise_lfsr: u32,
    pub prev_msb: bool,
    /// Cycles until the next noise-register cell drifts to one while TEST is
    /// held; zero once the register is all ones or TEST is clear.
    shift_register_reset: u32,
    /// The 12-bit waveform DAC input, latched each cycle. With no waveform
    /// selected the input floats and keeps its last value; OSC3 reads it.
    output: u16,
    /// Cycles until the floating DAC input loses its next bit; zero while a
    /// waveform is selected or once the input has faded to zero.
    floating_output_ttl: u32,
}

impl Voice {
    #[must_use]
    pub fn new() -> Self {
        Self {
            accumulator: 0,
            frequency: 0,
            pulse_width: 0,
            control: 0,
            noise_lfsr: NOISE_LFSR_SEED,
            prev_msb: false,
            shift_register_reset: 0,
            output: 0,
            floating_output_ttl: 0,
        }
    }

    /// Write the control register, applying the TEST-bit edges.
    ///
    /// TEST rising zeros the accumulator and starts the noise register's
    /// drift towards all ones. TEST falling completes the shift that TEST
    /// left half done: the SRAM write is enabled, so the register shifts once
    /// with `bit0 = (bit22 | TEST) ^ bit17 = !bit17`. Per reSID `wave.cc`
    /// `writeCONTROL_REG`; the datasheet (MOS 6581, Test bit 3) only says TEST
    /// "resets and locks Oscillator 1 at zero" and resets the noise output.
    ///
    /// Before that shift, a release from a waveform combined with noise may
    /// first write the selector output into the register's tapped bits; see
    /// [`writes_back_on_test_release`].
    ///
    /// Deselecting every waveform leaves the DAC input floating: it holds its
    /// last value and then fades (see [`Self::clock_output`]).
    pub fn write_control(&mut self, value: u8, model: SidModel) {
        let test_prev = self.control & CONTROL_TEST != 0;
        let test = value & CONTROL_TEST != 0;
        let waveform_prev = self.control >> 4;
        self.control = value;

        if value >> 4 == 0 && waveform_prev != 0 {
            self.floating_output_ttl = match model {
                SidModel::Mos6581 => FLOATING_OUTPUT_TTL_START_6581,
                SidModel::Mos8580 => FLOATING_OUTPUT_TTL_START_8580,
            };
        }

        if !test_prev && test {
            self.accumulator = 0;
            self.shift_register_reset = match model {
                SidModel::Mos6581 => SHIFT_REGISTER_RESET_START_6581,
                SidModel::Mos8580 => SHIFT_REGISTER_RESET_START_8580,
            };
        } else if test_prev && !test {
            if writes_back_on_test_release(waveform_prev, value >> 4, model) {
                self.write_back_noise(self.output);
            }
            let bit0 = (!self.noise_lfsr >> 17) & 1;
            self.noise_lfsr = ((self.noise_lfsr << 1) | bit0) & NOISE_LFSR_MASK;
            self.shift_register_reset = 0;
        }
    }

    /// Advance the oscillator one cycle. While TEST is held the accumulator
    /// stays at zero and the noise register drifts towards all ones.
    pub fn clock_accumulator(&mut self, model: SidModel) {
        if self.control & CONTROL_TEST != 0 {
            self.accumulator = 0;
            if self.shift_register_reset != 0 {
                self.shift_register_reset -= 1;
                if self.shift_register_reset == 0 {
                    self.shift_register_bitfade(model);
                }
            }
            return;
        }

        self.accumulator = self.accumulator.wrapping_add(u32::from(self.frequency)) & 0x00FF_FFFF;
    }

    /// One step of the TEST-held drift: bit 0 reaches one and every set bit
    /// pulls its upper neighbour up, so the ones fill upwards to `0x7FFFFF`.
    /// reSID `shiftreg_bitfade`, masked to the 23-bit register (reSID leaves
    /// bit 23 to accumulate, which only keeps its timer re-arming).
    fn shift_register_bitfade(&mut self, model: SidModel) {
        self.noise_lfsr |= 1;
        self.noise_lfsr = (self.noise_lfsr | (self.noise_lfsr << 1)) & NOISE_LFSR_MASK;
        if self.noise_lfsr != NOISE_LFSR_MASK {
            self.shift_register_reset = match model {
                SidModel::Mos6581 => SHIFT_REGISTER_RESET_BIT_6581,
                SidModel::Mos8580 => SHIFT_REGISTER_RESET_BIT_8580,
            };
        }
    }

    pub fn clock_noise(&mut self) {
        let msb19 = self.accumulator & (1 << 19) != 0;
        let prev19 = self.accumulator.wrapping_sub(u32::from(self.frequency)) & (1 << 19) != 0;

        if msb19 && !prev19 {
            let bit17 = (self.noise_lfsr >> 17) & 1;
            let bit22 = (self.noise_lfsr >> 22) & 1;
            let feedback = bit17 ^ bit22;
            self.noise_lfsr = ((self.noise_lfsr << 1) | feedback) & 0x7F_FFFF;
        }
    }

    pub fn apply_sync(&mut self, source_prev_msb: bool, source_curr_msb: bool) {
        if source_curr_msb && !source_prev_msb {
            self.accumulator = 0;
        }
    }

    /// The latched 12-bit waveform DAC input: what the voice feeds its DAC
    /// and what OSC3 reads (top 8 bits) for voice 3.
    #[must_use]
    pub const fn output(&self) -> u16 {
        self.output
    }

    /// Latch this cycle's DAC input. With a waveform selected it is the
    /// generated waveform. With none selected the input floats: it keeps its
    /// last value, and once the hold time runs out each step ANDs it with
    /// itself shifted right, so the ones drain from the bottom until it reads
    /// zero. Per reSID `wave.h` `set_waveform_output` and `wave.cc`
    /// `wave_bitfade`; the "SID vicious" 8-bit digi method and OSC3 reads
    /// after deselecting a waveform depend on it (VICE `testprogs/SID/
    /// bitfade`, `osc3-wave0`). Same mechanism on the 6581 and 8580; the 8580
    /// holds about 25 times longer.
    pub fn clock_output(&mut self, ring_mod_source_msb: bool, model: SidModel) {
        if self.control >> 4 != 0 {
            self.latch_output(ring_mod_source_msb, model);
        } else if self.floating_output_ttl != 0 {
            self.floating_output_ttl -= 1;
            if self.floating_output_ttl == 0 {
                self.output &= self.output >> 1;
                if self.output != 0 {
                    self.floating_output_ttl = match model {
                        SidModel::Mos6581 => FLOATING_OUTPUT_TTL_BIT_6581,
                        SidModel::Mos8580 => FLOATING_OUTPUT_TTL_BIT_8580,
                    };
                }
            }
        }
    }

    /// Latch the generated waveform as the DAC input now (a control write
    /// selecting a waveform takes effect at once, as in reSID).
    ///
    /// With noise combined with another waveform, and TEST clear, the
    /// selector output is also written back into the noise register (see
    /// [`Self::write_back_noise`]).
    pub fn latch_output(&mut self, ring_mod_source_msb: bool, model: SidModel) {
        self.output = self.waveform_output(ring_mod_source_msb, model);
        if self.control >> 4 > 0x08 && self.control & CONTROL_TEST == 0 {
            self.write_back_noise(self.output);
        }
    }

    /// Pull down each tapped noise-register bit whose waveform DAC input is
    /// low.
    ///
    /// The waveform selector connects each noise output straight to its DAC
    /// input, and that same node is the input of the next register bit. When
    /// noise is combined with another waveform, a zero from the other
    /// waveform drives the node low and overwrites the register bit; a one
    /// leaves it alone. So the zeros accumulate, the shifts carry them up to
    /// the feedback taps (bits 22 and 17), and the register locks at zero:
    /// noise falls silent and stays silent, even alone, until TEST lets the
    /// cells drift back to ones. Per reSID `wave.h` `write_shift_register`
    /// (die-photo analysis) and reSIDfp's `get_noise_writeback`; VICE
    /// `testprogs/SID` `noisewriteback` and `wb_testsuite` check it on real
    /// 6581s and 8580s. Same on both models.
    ///
    /// reSID skips the write on the one cycle of its two-cycle shift
    /// pipeline where the bits are latched (`shift_pipeline != 1`); this
    /// voice shifts on the same cycle bit 19 rises (#1606), so it writes
    /// every cycle.
    fn write_back_noise(&mut self, output: u16) {
        let mask = NOISE_TAPS
            .iter()
            .fold(NOISE_LFSR_MASK, |mask, &(reg_bit, dac_bit)| {
                if output & (1 << dac_bit) == 0 {
                    mask & !(1 << reg_bit)
                } else {
                    mask
                }
            });
        self.noise_lfsr &= mask;
    }

    /// The waveform the generator drives this cycle, before the floating-
    /// input behaviour: zero when no waveform is selected.
    #[must_use]
    pub fn waveform_output(&self, ring_mod_source_msb: bool, model: SidModel) -> u16 {
        let waveform_bits = (self.control >> 4) & 0x0F;

        if waveform_bits == 0 {
            return 0;
        }

        // TEST bit (control bit 3) holds pulse output HIGH. Per 6581 datasheet.
        let test_bit = self.control & CONTROL_TEST != 0;

        let tri12 = self.triangle_output(ring_mod_source_msb);
        let saw12 = ((self.accumulator >> 12) & 0xFFF) as u16;
        // The 12-bit comparator drives the pulse line high once the upper
        // accumulator bits reach the pulse width: high while `acc >= PW`.
        // reSID `wave.h` (`pulse_output = (accumulator >> 12) >= pw`), reSIDfp
        // alike, and VICE `testprogs/SID/osc3-wave0` on hardware: PW $000
        // reads OSC3 $FF, PW $FFF reads $00. The datasheet's "0 or $FFF ...
        // constant DC" holds for either polarity, so it cannot settle this.
        let pulse12 = if test_bit {
            0x0FFF
        } else {
            let pw12 = self.pulse_width & 0x0FFF;
            let acc12 = ((self.accumulator >> 12) & 0x0FFF) as u16;
            if acc12 >= pw12 { 0x0FFF } else { 0x0000 }
        };
        let noise12 = self.noise_output();

        // Per reSID `wave.h` `set_waveform_output`: the triangle, sawtooth or
        // combined-waveform sample, ANDed with the pulse line and the noise
        // output when those are selected. Combinations of two or more of
        // triangle, sawtooth and pulse read the sampled OSC3 table for the
        // chip model (see `combined_wave_tables`), indexed by the
        // ring-substituted accumulator as reSID indexes every table. Noise
        // combined with one other waveform is the plain AND, as in reSID.
        let idx = ((self.ring_modulated_accumulator(ring_mod_source_msb) >> 12) & 0x0FFF) as usize;
        let wave = match (waveform_bits & 0x07, model) {
            (0x01, _) => tri12,
            (0x02, _) => saw12,
            (0x03, SidModel::Mos6581) => COMBINED_TRI_SAW_6581[idx],
            (0x03, SidModel::Mos8580) => COMBINED_TRI_SAW_8580[idx],
            (0x05, SidModel::Mos6581) => COMBINED_P_T_6581[idx],
            (0x05, SidModel::Mos8580) => COMBINED_P_T_8580[idx],
            (0x06, SidModel::Mos6581) => COMBINED_PS_6581[idx],
            (0x06, SidModel::Mos8580) => COMBINED_PS_8580[idx],
            (0x07, SidModel::Mos6581) => COMBINED_PST_6581[idx],
            (0x07, SidModel::Mos8580) => COMBINED_PST_8580[idx],
            // Pulse alone, or noise alone: the masks below give the output.
            _ => 0x0FFF,
        };
        let pulse_mask = if waveform_bits & 0x04 != 0 {
            pulse12
        } else {
            0x0FFF
        };
        let noise_mask = if waveform_bits & 0x08 != 0 {
            noise12
        } else {
            0x0FFF
        };
        let output = wave & pulse_mask & noise_mask;
        if waveform_bits & 0x0C == 0x0C {
            noise_pulse(output, model)
        } else {
            output
        }
    }

    /// The accumulator as the triangle XOR logic sees it.
    ///
    /// Ring modulation substitutes this voice's MSB with
    /// `MSB EOR NOT source-MSB`. Die analysis gives the XOR input as
    /// `TriXOR = !Saw & ((!V3 & Ring) ^ bit23)`, so the substitution is
    /// suppressed when sawtooth is co-selected (sawtooth blocks the MSB out of
    /// the EOR). Per reSID `wave.cc` `writeCONTROL_REG` (`ring_msb_mask`) and
    /// `wave.h` `set_waveform_output` (`accumulator ^ (~sync_source &
    /// ring_msb_mask)`), and VICE `testprogs/SID/ringmod`, which reads `$FF`
    /// from OSC3 for a ring-modulated triangle with both oscillators at zero.
    /// reSIDfp shares the expression. The datasheet only says RING "replaces
    /// the Triangle waveform output ... with a Ring Modulated combination";
    /// it does not give the polarity. Same on the 6581 and 8580.
    fn ring_modulated_accumulator(&self, ring_mod_source_msb: bool) -> u32 {
        let ring = self.control & 0x04 != 0;
        let saw = self.control & 0x20 != 0;
        if ring && !saw && !ring_mod_source_msb {
            self.accumulator ^ 0x0080_0000
        } else {
            self.accumulator
        }
    }

    /// Triangle connects accumulator bits 22..12 (inverted while the MSB is
    /// set) to DAC inputs 11..1; DAC bit 0 is left at zero. Per the die
    /// analysis in reSID `wave.h` ("DAC bit 0 = 0, DAC bit n = accumulator
    /// bit n - 1") and its triangle table (`... >> 11) & 0xffe`). Same on the
    /// 6581 and 8580.
    fn triangle_output(&self, ring_mod_source_msb: bool) -> u16 {
        let tri = self.ring_modulated_accumulator(ring_mod_source_msb);
        let value = if tri & 0x0080_0000 != 0 {
            (tri ^ 0x007F_FFFF) >> 11
        } else {
            tri >> 11
        };
        (value & 0x0FFE) as u16
    }

    /// The noise waveform: shift-register bits 20, 18, 14, 11, 9, 5, 2 and 0
    /// drive DAC inputs 11..4; the low four DAC inputs are grounded. Same on
    /// the 6581 and 8580.
    ///
    /// These are the positions the die photographs give, per reSID 1.0
    /// `wave.h` (`set_noise_output`) and reSIDfp `WaveformGenerator.cpp`,
    /// which agree. reSID 0.16 and earlier sampled 22, 20, 16, 13, 11, 7, 4
    /// and 2, the same pattern two shifts later. The difference shows the
    /// moment a shift lands: from an all-ones register one TEST pulse shifts
    /// a zero into bit 0, and VICE `testprogs/SID` `wb_testsuite` and
    /// `noisewriteback` read OSC3 `$FE` on real 6581s and 8580s, which only
    /// the bit-0 tap gives. The same bits take the combined-waveform
    /// write-back ([`NOISE_TAPS`]).
    fn noise_output(&self) -> u16 {
        let lfsr = self.noise_lfsr;
        NOISE_TAPS.iter().fold(0, |out, &(reg_bit, dac_bit)| {
            out | ((((lfsr >> reg_bit) & 1) as u16) << dac_bit)
        })
    }

    #[must_use]
    pub fn msb(&self) -> bool {
        self.accumulator & 0x0080_0000 != 0
    }
}

/// Whether releasing TEST, from `waveform_prev` to `waveform` (control bits
/// 7-4), writes the selector output into the noise register before the
/// release's shift.
///
/// While TEST is held the register bits are interconnected for the first
/// phase of a shift and the output does not reach them. On release the
/// second phase completes, and the output of a combined waveform may land
/// in the latched bits first. Which transitions do so is measured, not
/// derived. The rules are reSIDfp `WaveformGenerator.cpp` `do_writeback`,
/// whose comments name the VICE `testprogs/SID` `wb_testsuite` and
/// `noisewriteback` programs (real 6581 and 8580 samplings) each rule
/// fixes, except for releases from noise+pulse on the 8580. There reSIDfp
/// never writes back, but reSID `wave.cc` `do_pre_writeback` writes back
/// into noise+triangle and noise+pulse+sawtooth, and the real-8580
/// `wb_testsuite` programs `C_to_9_new` and `C_to_E_new` side with reSID
/// while `C_to_A_new` agrees with both.
const fn writes_back_on_test_release(waveform_prev: u8, waveform: u8, model: SidModel) -> bool {
    // No combined waveform before, or no noise after.
    if waveform_prev <= 0x8 || waveform < 0x8 {
        return false;
    }
    // Back to noise alone writes back only from all four waveforms.
    if waveform == 0x8 && waveform_prev != 0xF {
        return false;
    }
    // On the 6581, a swap between triangle and sawtooth does not.
    let swaps_tri_saw = (waveform_prev & 0x3 == 0x1 && waveform & 0x3 == 0x2)
        || (waveform_prev & 0x3 == 0x2 && waveform & 0x3 == 0x1);
    if matches!(model, SidModel::Mos6581) && swaps_tri_saw {
        return false;
    }
    // From noise+pulse, only the 8580 into noise+triangle or
    // noise+pulse+sawtooth; into noise+pulse, never.
    if waveform_prev == 0xC {
        return matches!(model, SidModel::Mos8580) && (waveform == 0x9 || waveform == 0xE);
    }
    waveform != 0xC
}

/// Noise combined with pulse pulls further bits down than the AND gives,
/// differently on each model. reSID `wave.h` (`noise_pulse6581`,
/// `noise_pulse8580`), applied after the AND to every combination that
/// includes both noise and pulse. On a 6581 an output below `$F00` reads
/// zero and a bit survives only if its two lower neighbours are set; an 8580
/// keeps a bit only if its lower neighbour is set and reads `$FC0` from
/// `$FC0` up. VICE `testprogs/SID/wf12nsr` reads OSC3 252 (`$FC`) for
/// noise+pulse with TEST held on a real 6581.
const fn noise_pulse(output: u16, model: SidModel) -> u16 {
    match model {
        SidModel::Mos6581 => {
            if output < 0xF00 {
                0x000
            } else {
                output & (output << 1) & (output << 2)
            }
        }
        SidModel::Mos8580 => {
            if output < 0xFC0 {
                output & (output << 1)
            } else {
                0xFC0
            }
        }
    }
}

impl Default for Voice {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // Control-register waveform-select bits (control >> 4).
    const TRI: u8 = 0x10;
    const SAW: u8 = 0x20;
    const PULSE: u8 = 0x40;
    const NOISE: u8 = 0x80;
    const RING: u8 = 0x04;
    const TEST: u8 = 0x08;

    #[test]
    fn sawtooth_is_the_upper_12_accumulator_bits() {
        let mut v = Voice::new();
        v.control = SAW;
        v.accumulator = 0x00AB_C000;
        assert_eq!(v.waveform_output(false, SidModel::Mos6581), 0xABC);
    }

    #[test]
    fn pulse_is_high_from_the_pulse_width_up_and_low_below() {
        for model in [SidModel::Mos6581, SidModel::Mos8580] {
            let mut v = Voice::new();
            v.control = PULSE;
            v.pulse_width = 0x800;
            v.accumulator = 0x0040_0000; // acc12 = 0x400 < 0x800
            assert_eq!(v.waveform_output(false, model), 0x0000);
            v.accumulator = 0x0080_0000; // acc12 = 0x800 == PW
            assert_eq!(v.waveform_output(false, model), 0x0FFF);
            v.accumulator = 0x00C0_0000; // acc12 = 0xC00 > 0x800
            assert_eq!(v.waveform_output(false, model), 0x0FFF);
        }
    }

    #[test]
    fn pulse_width_extremes_match_the_osc3_wave0_testprog() {
        // VICE testprogs/SID/osc3-wave0: PW $FFF reads OSC3 $00 and PW $000
        // reads $FF with the oscillator stopped at zero.
        let mut v = Voice::new();
        v.control = PULSE;
        v.accumulator = 0;
        v.pulse_width = 0xFFF;
        assert_eq!(v.waveform_output(false, SidModel::Mos6581), 0x0000);
        v.pulse_width = 0x000;
        assert_eq!(v.waveform_output(false, SidModel::Mos6581), 0x0FFF);
    }

    #[test]
    fn no_waveform_selected_outputs_zero() {
        let v = Voice::new();
        assert_eq!(v.waveform_output(false, SidModel::Mos6581), 0);
    }

    #[test]
    fn test_bit_holds_pulse_high_and_zeros_the_accumulator() {
        let mut v = Voice::new();
        v.accumulator = 0x0012_3456;
        v.pulse_width = 0x800;
        v.write_control(PULSE | TEST, SidModel::Mos6581);
        assert_eq!(v.accumulator, 0, "TEST rising zeros the accumulator");
        v.frequency = 0x1234;
        v.clock_accumulator(SidModel::Mos6581);
        assert_eq!(v.accumulator, 0, "TEST holds the accumulator at zero");
        assert_eq!(v.waveform_output(false, SidModel::Mos6581), 0x0FFF);
    }

    #[test]
    fn deselected_waveform_holds_then_fades_the_dac_input() {
        for (model, start, step) in [
            (SidModel::Mos6581, 182_000_u32, 1_500_u32),
            (SidModel::Mos8580, 4_400_000, 50_000),
        ] {
            let mut v = Voice::new();
            v.write_control(PULSE, model);
            v.clock_output(false, model); // PW 0: pulse high
            assert_eq!(v.output(), 0xFFF);

            v.write_control(0x00, model);
            for _ in 0..start - 1 {
                v.clock_output(false, model);
            }
            assert_eq!(v.output(), 0xFFF, "{model:?} holds until the TTL");
            v.clock_output(false, model);
            assert_eq!(v.output(), 0x7FF, "{model:?} first fade step");
            for _ in 0..step {
                v.clock_output(false, model);
            }
            assert_eq!(v.output(), 0x3FF, "{model:?} second fade step");

            let mut cycles = 0_u32;
            while v.output() != 0 {
                v.clock_output(false, model);
                cycles += 1;
            }
            assert_eq!(cycles, 10 * step, "{model:?} drains one bit per step");
        }
    }

    #[test]
    fn reselecting_a_waveform_ends_the_float() {
        let mut v = Voice::new();
        v.write_control(PULSE, SidModel::Mos6581);
        v.clock_output(false, SidModel::Mos6581);
        v.write_control(0x00, SidModel::Mos6581);
        v.clock_output(false, SidModel::Mos6581);
        assert_eq!(v.output(), 0xFFF);
        v.write_control(SAW, SidModel::Mos6581);
        v.clock_output(false, SidModel::Mos6581);
        assert_eq!(v.output(), 0x000, "sawtooth at accumulator zero");
    }

    #[test]
    fn test_bit_does_not_reseed_the_noise_register_at_once() {
        // The SRAM cells drift towards one; a brief TEST pulse leaves most of
        // the register as it was.
        let mut v = Voice::new();
        v.noise_lfsr = 0x0000_1234;
        v.write_control(NOISE | TEST, SidModel::Mos6581);
        for _ in 0..100 {
            v.clock_accumulator(SidModel::Mos6581);
        }
        assert_eq!(v.noise_lfsr, 0x0000_1234);
    }

    /// Hold TEST from a cleared noise register and return the cycle count
    /// at which it first reads all ones.
    fn cycles_to_all_ones(model: SidModel) -> u32 {
        let mut v = Voice::new();
        v.noise_lfsr = 0;
        v.write_control(NOISE | TEST, model);
        let mut cycles = 0;
        while v.noise_lfsr != 0x7F_FFFF {
            v.clock_accumulator(model);
            cycles += 1;
            assert!(cycles < 20_000_000, "noise register never filled");
        }
        cycles
    }

    #[test]
    fn held_test_bit_ramps_the_noise_register_to_all_ones() {
        let mut v = Voice::new();
        v.noise_lfsr = 0;
        v.write_control(NOISE | TEST, SidModel::Mos6581);
        for _ in 0..34_999 {
            v.clock_accumulator(SidModel::Mos6581);
        }
        assert_eq!(v.noise_lfsr, 0, "nothing drifts before the start delay");
        v.clock_accumulator(SidModel::Mos6581);
        assert_eq!(v.noise_lfsr, 0b11, "first step sets bit 0 and smears up");
        for _ in 0..1_000 {
            v.clock_accumulator(SidModel::Mos6581);
        }
        assert_eq!(v.noise_lfsr, 0b111);

        // Start delay plus 21 further steps fill the 23-bit register.
        assert_eq!(cycles_to_all_ones(SidModel::Mos6581), 35_000 + 21 * 1_000);
        assert_eq!(
            cycles_to_all_ones(SidModel::Mos8580),
            2_519_864 + 21 * 315_000
        );
    }

    #[test]
    fn test_bit_falling_shifts_in_the_inverse_of_bit_17() {
        let mut v = Voice::new();
        v.noise_lfsr = 0x0000_0001; // bit 17 clear -> bit0 = 1
        v.write_control(NOISE | TEST, SidModel::Mos6581);
        v.write_control(NOISE, SidModel::Mos6581);
        assert_eq!(v.noise_lfsr, 0b11);

        v.noise_lfsr = 1 << 17; // bit 17 set -> bit0 = 0
        v.write_control(NOISE | TEST, SidModel::Mos6581);
        v.write_control(NOISE, SidModel::Mos6581);
        assert_eq!(v.noise_lfsr, 1 << 18);

        // Writes that leave TEST unchanged do not shift.
        v.write_control(NOISE | 0x01, SidModel::Mos6581);
        assert_eq!(v.noise_lfsr, 1 << 18);
    }

    #[test]
    fn ring_mod_substitutes_msb_eor_not_source_msb() {
        // TriXOR = !Saw & ((!V3 & Ring) ^ bit23): with both MSBs clear the
        // triangle is inverted (VICE testprogs/SID/ringmod reads OSC3 = $FF),
        // and a set source MSB leaves it unfolded.
        for model in [SidModel::Mos6581, SidModel::Mos8580] {
            let mut v = Voice::new();
            v.control = TRI | RING;
            v.accumulator = 0;
            assert_eq!(
                v.waveform_output(false, model),
                0xFFE,
                "clear source MSB inverts the triangle ({model:?})"
            );
            assert_eq!(
                v.waveform_output(true, model),
                0x000,
                "set source MSB leaves the triangle unfolded ({model:?})"
            );
        }
    }

    #[test]
    fn triangle_dac_bit_zero_is_grounded() {
        // Accumulator bit 11 is below the 11 bits triangle routes to the DAC.
        let mut v = Voice::new();
        v.control = TRI;
        v.accumulator = 0x0000_0800;
        assert_eq!(v.waveform_output(false, SidModel::Mos6581), 0x000);
        v.accumulator = 0x0000_1800; // bit 12 -> DAC bit 1
        assert_eq!(v.waveform_output(false, SidModel::Mos6581), 0x002);
        v.accumulator = 0x0080_0000; // falling half starts at the top
        assert_eq!(v.waveform_output(false, SidModel::Mos8580), 0xFFE);
    }

    #[test]
    fn ring_mod_without_ring_bit_ignores_the_source() {
        let mut v = Voice::new();
        v.control = TRI;
        v.accumulator = 0;
        assert_eq!(v.waveform_output(false, SidModel::Mos6581), 0x000);
        assert_eq!(v.waveform_output(true, SidModel::Mos6581), 0x000);
    }

    #[test]
    fn sawtooth_blocks_the_ring_mod_substitution() {
        // With sawtooth co-selected the MSB never reaches the EOR, so the
        // tri+saw table is indexed by the raw accumulator whatever the source.
        let mut v = Voice::new();
        v.control = TRI | SAW | RING;
        v.accumulator = 0x0055_5000;
        let raw = COMBINED_TRI_SAW_6581[0x555];
        assert_eq!(v.waveform_output(false, SidModel::Mos6581), raw);
        assert_eq!(v.waveform_output(true, SidModel::Mos6581), raw);
    }

    #[test]
    fn ring_mod_flips_the_pulse_triangle_table_index() {
        // reSID indexes every combined table with the ring-substituted
        // accumulator; pulse+triangle has no sawtooth, so RING applies.
        let mut v = Voice::new();
        v.control = TRI | PULSE | RING;
        v.pulse_width = 0x000; // pulse held high
        v.accumulator = 0x009F_F000;
        assert_eq!(
            v.waveform_output(false, SidModel::Mos6581),
            COMBINED_P_T_6581[0x1FF],
            "clear source MSB reads the opposite half of the table"
        );
        assert_ne!(COMBINED_P_T_6581[0x1FF], COMBINED_P_T_6581[0x9FF]);
        assert_eq!(
            v.waveform_output(true, SidModel::Mos6581),
            COMBINED_P_T_6581[0x9FF]
        );
    }

    #[test]
    fn hard_sync_zeros_the_accumulator_only_on_the_source_rising_edge() {
        let mut v = Voice::new();
        v.accumulator = 0x0012_3456;
        v.apply_sync(true, true); // no rising edge
        assert_eq!(v.accumulator, 0x0012_3456);
        v.apply_sync(false, true); // rising edge
        assert_eq!(v.accumulator, 0);
    }

    #[test]
    fn combined_tri_saw_uses_the_6581_sampled_table() {
        let mut v = Voice::new();
        v.control = TRI | SAW;
        v.accumulator = 0x0055_5000;
        let idx = ((v.accumulator >> 12) & 0x0FFF) as usize;
        assert_eq!(
            v.waveform_output(false, SidModel::Mos6581),
            COMBINED_TRI_SAW_6581[idx],
            "6581 combined waveform reads the sampled ROM table, not a bitwise AND"
        );
    }

    /// reSID's OSC3 samples for one model, in table order `__ST`, `_P_T`,
    /// `_PS_`, `_PST` (control waveform bits 3, 5, 6, 7).
    fn sampled_tables(model: SidModel) -> [&'static [u8; 4096]; 4] {
        match model {
            SidModel::Mos6581 => [
                include_bytes!("../data/wave6581__ST.dat"),
                include_bytes!("../data/wave6581_P_T.dat"),
                include_bytes!("../data/wave6581_PS_.dat"),
                include_bytes!("../data/wave6581_PST.dat"),
            ],
            SidModel::Mos8580 => [
                include_bytes!("../data/wave8580__ST.dat"),
                include_bytes!("../data/wave8580_P_T.dat"),
                include_bytes!("../data/wave8580_PS_.dat"),
                include_bytes!("../data/wave8580_PST.dat"),
            ],
        }
    }

    #[test]
    fn combined_waveforms_read_each_models_sampled_osc3_table() {
        // Every accumulator position of every non-noise combination, with
        // the pulse held high (PW 0), reads reSID's 8-bit OSC3 sample for
        // that model shifted up to the 12-bit DAC input.
        for model in [SidModel::Mos6581, SidModel::Mos8580] {
            for (waveform, table) in [0x30, 0x50, 0x60, 0x70]
                .into_iter()
                .zip(sampled_tables(model))
            {
                let mut v = Voice::new();
                v.control = waveform;
                v.pulse_width = 0;
                for ix in 0..4096_u32 {
                    v.accumulator = ix << 12;
                    assert_eq!(
                        v.waveform_output(true, model),
                        u16::from(table[ix as usize]) << 4,
                        "{model:?} waveform ${waveform:02X} index ${ix:03X}"
                    );
                }
            }
        }
    }

    #[test]
    fn mos8580_tri_saw_is_not_a_bitwise_and() {
        // At $8E4 triangle AND sawtooth is $824; the sampled 8580 reads 0.
        let mut v = Voice::new();
        v.control = TRI | SAW;
        v.accumulator = 0x008E_4000;
        assert_eq!(v.waveform_output(false, SidModel::Mos8580), 0x000);
        // At $FFF the two models' samples differ: $FF0 on the 8580, $7F0 on
        // the 6581.
        v.accumulator = 0x00FF_F000;
        assert_eq!(v.waveform_output(false, SidModel::Mos8580), 0xFF0);
        assert_eq!(v.waveform_output(false, SidModel::Mos6581), 0x7F0);
    }

    #[test]
    fn noise_plus_pulse_reads_252_with_test_held() {
        // VICE testprogs/SID wf12nsr (bug #1037): voice 3 on noise+pulse with
        // TEST set and the noise register all ones settles at OSC3 252 on a
        // real 6581, not 255. reSID `noise_pulse6581`/`noise_pulse8580`.
        for model in [SidModel::Mos6581, SidModel::Mos8580] {
            let mut v = Voice::new();
            v.noise_lfsr = NOISE_LFSR_MASK;
            v.control = NOISE | PULSE | TEST;
            assert_eq!(v.waveform_output(false, model), 0xFC0, "{model:?}");
        }
    }

    #[test]
    fn noise_plus_pulse_pulls_bits_down_per_model() {
        let mut v = Voice::new();
        v.control = NOISE | PULSE;
        v.pulse_width = 0; // pulse high
        // Noise output $EA0 (bits 20, 18, 14, 9 and 2 set): the 6581 drops
        // anything below $F00 to zero; the 8580 ANDs it with itself shifted
        // up one.
        v.noise_lfsr = (1 << 20) | (1 << 18) | (1 << 14) | (1 << 9) | (1 << 2);
        assert_eq!(v.waveform_output(false, SidModel::Mos6581), 0x000);
        assert_eq!(v.waveform_output(false, SidModel::Mos8580), 0xEA0 & 0xD40);
        // Noise output $FB0: the 6581 keeps the bits whose two lower
        // neighbours are set; the 8580 again ANDs it with itself shifted up.
        v.noise_lfsr = (1 << 20) | (1 << 18) | (1 << 14) | (1 << 11) | (1 << 9) | (1 << 2) | 1;
        assert_eq!(
            v.waveform_output(false, SidModel::Mos6581),
            0xFB0 & 0xF60 & 0xEC0
        );
        assert_eq!(v.waveform_output(false, SidModel::Mos8580), 0xFB0 & 0xF60);
        // At $FC0 and above the 8580 reads $FC0.
        v.noise_lfsr = NOISE_LFSR_MASK;
        assert_eq!(v.waveform_output(false, SidModel::Mos8580), 0xFC0);
    }

    /// The register bits the noise waveform reads (see `NOISE_TAPS`).
    const TAP_BITS: u32 =
        (1 << 20) | (1 << 18) | (1 << 14) | (1 << 11) | (1 << 9) | (1 << 5) | (1 << 2) | 1;

    #[test]
    fn noise_combined_with_another_waveform_writes_its_zeros_back() {
        // Triangle at accumulator 0 is zero, so noise+triangle outputs zero
        // and pulls every tapped register bit down; untapped bits keep their
        // value. reSID `write_shift_register`.
        for model in [SidModel::Mos6581, SidModel::Mos8580] {
            let mut v = Voice::new();
            v.noise_lfsr = NOISE_LFSR_MASK;
            v.write_control(NOISE | TRI, model);
            v.latch_output(false, model);
            assert_eq!(v.output(), 0);
            assert_eq!(v.noise_lfsr, NOISE_LFSR_MASK & !TAP_BITS, "{model:?}");
        }
    }

    #[test]
    fn write_back_only_clears_bits_the_output_drives_low() {
        // Sawtooth $A50 ANDed with all-ones noise ($FF0) gives $A50: DAC bits
        // 11, 9, 6 and 4 high, so register bits 20, 14, 5 and 0 survive and
        // 18, 11, 9 and 2 are pulled down.
        let mut v = Voice::new();
        v.noise_lfsr = NOISE_LFSR_MASK;
        v.write_control(NOISE | SAW, SidModel::Mos6581);
        v.accumulator = 0x00A5_0000;
        v.latch_output(false, SidModel::Mos6581);
        assert_eq!(v.output(), 0xA50);
        let cleared = (1 << 18) | (1 << 11) | (1 << 9) | (1 << 2);
        assert_eq!(v.noise_lfsr, NOISE_LFSR_MASK & !cleared);
    }

    #[test]
    fn noise_alone_and_test_held_do_not_write_back() {
        let mut v = Voice::new();
        v.noise_lfsr = 0x0012_3456;
        v.write_control(NOISE, SidModel::Mos6581);
        v.latch_output(false, SidModel::Mos6581);
        assert_eq!(v.noise_lfsr, 0x0012_3456, "noise alone");

        // With TEST held the register cells are interconnected for the shift
        // and the selector output does not reach them.
        v.write_control(NOISE | TRI | TEST, SidModel::Mos6581);
        v.latch_output(false, SidModel::Mos6581);
        assert_eq!(v.output(), 0);
        assert_eq!(v.noise_lfsr, 0x0012_3456, "TEST held");
    }

    /// Clock one voice as the SID does each cycle, without a ring or sync
    /// source.
    fn clock(v: &mut Voice, model: SidModel) {
        v.clock_accumulator(model);
        v.clock_noise();
        v.clock_output(false, model);
    }

    #[test]
    fn combined_noise_locks_the_register_at_zero_until_test_refills_it() {
        for model in [SidModel::Mos6581, SidModel::Mos8580] {
            let mut v = Voice::new();
            v.frequency = 0x2000;
            v.write_control(NOISE | PULSE, model);
            v.pulse_width = 0x800;
            // Pulse is low for half of each cycle, so every tap is pulled
            // down; the shifts carry the zeros up until the feedback, bits
            // 22 and 17, is zero too.
            for _ in 0..200_000 {
                clock(&mut v, model);
            }
            assert_eq!(v.noise_lfsr, 0, "{model:?} register locked at zero");

            // Back to noise alone: still silent, because zero feeds back zero.
            v.write_control(NOISE, model);
            for _ in 0..200_000 {
                clock(&mut v, model);
            }
            assert_eq!(v.noise_lfsr, 0, "{model:?} stays locked");
            assert_eq!(v.output(), 0);

            // Only TEST revives it: held long enough, the cells drift to ones
            // and noise resumes on release.
            v.write_control(NOISE | TEST, model);
            let mut held = 0_u32;
            while v.noise_lfsr != NOISE_LFSR_MASK {
                clock(&mut v, model);
                held += 1;
                assert!(held < 20_000_000, "{model:?} never refilled");
            }
            v.write_control(NOISE, model);
            let mut seen = 0_u16;
            for _ in 0..200_000 {
                clock(&mut v, model);
                seen |= v.output();
            }
            assert_eq!(seen, 0xFF0, "{model:?} noise runs again");
        }
    }

    #[test]
    fn test_release_write_back_follows_the_measured_transitions() {
        // Release TEST from waveform `from` (with TEST) to `to`, starting
        // from an all-ones register, and report whether the release wrote
        // the output's zeros back before shifting. Without a write-back the
        // release only shifts in !bit17 = 0, giving $7FFFFE.
        fn writes_back(from: u8, to: u8, model: SidModel) -> bool {
            let mut v = Voice::new();
            v.noise_lfsr = NOISE_LFSR_MASK;
            v.write_control(from | TEST, model);
            v.latch_output(false, model);
            assert_ne!(v.output(), 0xFF0, "${from:02X} drives a tap low");
            v.write_control(to, model);
            v.noise_lfsr != 0x7F_FFFE
        }
        use SidModel::{Mos6581, Mos8580};
        // No noise after: nothing to write back.
        assert!(!writes_back(NOISE | TRI, TRI, Mos6581));
        // Back to noise alone only from all four waveforms.
        assert!(!writes_back(NOISE | TRI, NOISE, Mos8580));
        assert!(writes_back(NOISE | PULSE | SAW | TRI, NOISE, Mos8580));
        // Into noise+pulse never writes back; out of it only the 8580 into
        // noise+triangle or noise+pulse+sawtooth.
        assert!(!writes_back(NOISE | TRI, NOISE | PULSE, Mos8580));
        assert!(!writes_back(NOISE | PULSE, NOISE | TRI, Mos6581));
        assert!(writes_back(NOISE | PULSE, NOISE | TRI, Mos8580));
        assert!(writes_back(NOISE | PULSE, NOISE | PULSE | SAW, Mos8580));
        assert!(!writes_back(NOISE | PULSE, NOISE | SAW, Mos8580));
        assert!(!writes_back(NOISE | PULSE, NOISE | PULSE | TRI, Mos8580));
        // The 6581 skips swaps between triangle and sawtooth; the 8580 does not.
        assert!(!writes_back(NOISE | TRI, NOISE | SAW, Mos6581));
        assert!(writes_back(NOISE | TRI, NOISE | SAW, Mos8580));
        assert!(writes_back(NOISE | SAW | TRI, NOISE | SAW | TRI, Mos6581));
    }

    #[test]
    fn noise_lfsr_advances_only_on_the_bit19_rising_edge() {
        let mut v = Voice::new();
        v.frequency = 1;
        v.accumulator = 0x0008_0000; // bit19 set, prev (acc-1) clear → rising
        let before = v.noise_lfsr;
        v.clock_noise();
        assert_ne!(v.noise_lfsr, before, "LFSR clocks on the rising edge");
        v.accumulator = 0x0008_0002; // bit19 stays set → no edge
        let held = v.noise_lfsr;
        v.clock_noise();
        assert_eq!(v.noise_lfsr, held, "no clock without an edge");
    }

    #[test]
    fn msb_reflects_accumulator_bit_23() {
        let mut v = Voice::new();
        v.accumulator = 0x0080_0000;
        assert!(v.msb());
        v.accumulator = 0x007F_FFFF;
        assert!(!v.msb());
    }

    #[test]
    fn noise_waveform_taps_register_bits_20_18_14_11_9_5_2_0() {
        let mut v = Voice::new();
        v.control = NOISE;
        for (bit, out) in [
            (20, 0x800),
            (18, 0x400),
            (14, 0x200),
            (11, 0x100),
            (9, 0x080),
            (5, 0x040),
            (2, 0x020),
            (0, 0x010),
        ] {
            v.noise_lfsr = 1 << bit;
            assert_eq!(
                v.waveform_output(false, SidModel::Mos6581),
                out,
                "bit {bit}"
            );
        }
        for bit in [22, 21, 19, 17, 16, 13, 7, 4] {
            v.noise_lfsr = 1 << bit;
            assert_eq!(v.waveform_output(false, SidModel::Mos6581), 0, "bit {bit}");
        }
    }

    #[test]
    fn test_falling_from_all_ones_reads_osc3_fe() {
        // VICE testprogs/SID wb_testsuite `8_to_8` and noisewriteback
        // `noise_writeback_test1` (real 6581 and 8580): with the register all
        // ones, one TEST pulse shifts in !bit17 = 0, and OSC3 reads $FE. Bit
        // 0 is a waveform tap, so the new zero shows at once.
        for model in [SidModel::Mos6581, SidModel::Mos8580] {
            let mut v = Voice::new();
            v.noise_lfsr = NOISE_LFSR_MASK;
            v.write_control(NOISE | TEST, model);
            v.write_control(NOISE, model);
            assert_eq!(v.waveform_output(false, model) >> 4, 0xFE, "{model:?}");
        }
    }

    #[test]
    fn noise_waveform_selects_lfsr_bits() {
        let mut v = Voice::new();
        v.control = NOISE;
        v.noise_lfsr = NOISE_LFSR_SEED; // all ones → all sampled bits set
        // Sampled into output bits 11..=4, so 0xFF0.
        assert_eq!(v.waveform_output(false, SidModel::Mos6581), 0xFF0);
    }
}
