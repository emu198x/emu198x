//! Audio generation for ZX Spectrum beeper emulation.
//!
//! The Spectrum beeper is a 1-bit audio device controlled by bit 4 of port 0xFE.
//! This module converts beeper transitions (recorded during CPU execution) into
//! audio samples suitable for playback.

/// Audio sample rate in Hz.
pub const SAMPLE_RATE: u32 = 44100;

/// Number of audio samples per frame at 50 FPS.
/// 44100 / 50 = 882
pub const SAMPLES_PER_FRAME: usize = 882;

/// T-states per frame (69888 for 48K Spectrum).
const T_STATES_PER_FRAME: u32 = 69888;

/// T-states per audio sample.
/// 69888 / 882 ≈ 79.24
const T_STATES_PER_SAMPLE: f32 = T_STATES_PER_FRAME as f32 / SAMPLES_PER_FRAME as f32;

/// Amplitude for beeper output (0.0 to 1.0 range, we use 0.5 for comfortable volume).
const AMPLITUDE: f32 = 0.5;

/// Generate audio samples from beeper transitions.
///
/// This function integrates the beeper level over each sample's T-state window
/// to produce smooth audio even when the beeper toggles faster than the sample rate.
///
/// # Arguments
/// * `transitions` - Slice of (t_state, level) pairs recorded during the frame
/// * `initial_level` - Beeper level at the start of the frame (before first transition)
/// * `samples` - Output buffer to fill with audio samples
pub fn generate_frame_samples(
    transitions: &[(u32, bool)],
    initial_level: bool,
    samples: &mut [f32],
) {
    let mut transition_idx = 0;
    let mut current_level = initial_level;

    for (sample_idx, sample) in samples.iter_mut().enumerate() {
        // Calculate the T-state window for this sample
        let t_start = sample_idx as f32 * T_STATES_PER_SAMPLE;
        let t_end = t_start + T_STATES_PER_SAMPLE;

        // Integrate beeper level over this sample's window
        let mut high_time = 0.0f32;
        let mut t_pos = t_start;

        // Process any transitions that fall within this sample's window
        while transition_idx < transitions.len() {
            let (trans_t, new_level) = transitions[transition_idx];
            let trans_t = trans_t as f32;

            if trans_t >= t_end {
                // Transition is beyond this sample's window
                break;
            }

            if trans_t > t_pos {
                // Accumulate time at current level before the transition
                if current_level {
                    high_time += trans_t - t_pos;
                }
                t_pos = trans_t;
            }

            current_level = new_level;
            transition_idx += 1;
        }

        // Accumulate remaining time at current level to end of window
        if current_level {
            high_time += t_end - t_pos;
        }

        // Convert to audio sample: ratio of high time to total time
        let ratio = high_time / T_STATES_PER_SAMPLE;
        // Map to -AMPLITUDE..+AMPLITUDE range
        *sample = (ratio * 2.0 - 1.0) * AMPLITUDE;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn silence_when_no_transitions_low() {
        let mut samples = [0.0f32; SAMPLES_PER_FRAME];
        generate_frame_samples(&[], false, &mut samples);

        // All samples should be -AMPLITUDE (beeper low)
        for sample in &samples {
            assert!((*sample - (-AMPLITUDE)).abs() < 0.001);
        }
    }

    #[test]
    fn full_volume_when_no_transitions_high() {
        let mut samples = [0.0f32; SAMPLES_PER_FRAME];
        generate_frame_samples(&[], true, &mut samples);

        // All samples should be +AMPLITUDE (beeper high)
        for sample in &samples {
            assert!((*sample - AMPLITUDE).abs() < 0.001);
        }
    }

    #[test]
    fn single_transition_mid_frame() {
        let mut samples = [0.0f32; SAMPLES_PER_FRAME];
        // Transition at halfway through the frame
        let transitions = vec![(T_STATES_PER_FRAME / 2, true)];
        generate_frame_samples(&transitions, false, &mut samples);

        // First half should be low, second half should be high
        let mid = SAMPLES_PER_FRAME / 2;
        for sample in &samples[..mid] {
            assert!(*sample < 0.0, "First half should be negative");
        }
        for sample in &samples[mid..] {
            assert!(*sample > 0.0, "Second half should be positive");
        }
    }
}
