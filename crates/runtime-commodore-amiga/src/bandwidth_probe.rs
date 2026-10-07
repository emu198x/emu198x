//! Explicit research diagnostic for the known host-decimation bandwidth gap.
//! Synthetic mixer gains isolate resampling from still-unresolved PWM phase.

use super::*;
use common_commodore_amiga::driver::AmigaDriver;
use std::f64::consts::TAU;

const WARMUP: usize = 2400;
const FRAMES: usize = 4800;

fn amplitude(samples: &[f32], frequency: u32) -> f64 {
    // Coherent 100 ms observation: all test and folded frequencies are kHz.
    // A Hann window also suppresses the residual filter startup transient.
    let mut re = 0.0;
    let mut im = 0.0;
    let mut weight = 0.0;
    let (frames, remainder) = samples.as_chunks::<2>();
    assert!(remainder.is_empty());
    for (n, sample) in frames.iter().enumerate() {
        let window = 0.5 - 0.5 * (TAU * n as f64 / FRAMES as f64).cos();
        let phase = TAU * f64::from(frequency) * n as f64 / 48_000.0;
        re += f64::from(sample[1]) * phase.cos() * window;
        im += f64::from(sample[1]) * phase.sin() * window;
        weight += window;
        assert_eq!(sample[0], 0.0, "right-only fixture leaked left");
    }
    2.0 * re.hypot(im) / weight
}

fn sweep<M: AmigaMachine + AmigaLiveAccess + AmigaDriver>(
    mut runtime: AmigaRuntime<M>,
) -> (usize, usize) {
    // Opposite held signed bytes on the two right-routed channels. Change
    // only mixer gains to construct a bipolar sine; do not claim a guest or
    // hardware PWM schedule. The real mixer, accumulator and filters execute.
    for (offset, value) in [
        (0xa6, 1000),
        (0xa8, 64),
        (0xaa, 0x4040),
        (0xd6, 1000),
        (0xd8, 64),
        (0xda, 0xc0c0),
    ] {
        AmigaDriver::dispatch_custom_write(runtime.machine_mut(), offset, value);
    }
    assert!(!runtime.machine.led_filter_engaged());
    runtime
        .machine
        .paula_mut()
        .set_audio_channel_gain(PaulaChannel::Channel3, 0.0);
    let input_amplitude = f64::from(runtime.machine.mix_audio_stereo().1);
    assert!(input_amplitude > 0.0);
    let mut count = 0;
    let mut failures = 0;
    for frequency in [1000_u32, 10_000, 20_000, 28_000, 55_000, 95_000] {
        let folded = frequency % AUDIO_SAMPLE_RATE_HZ;
        let folded = folded.min(AUDIO_SAMPLE_RATE_HZ - folded);
        for initial_phase in [0.0, 0.37, 0.91] {
            runtime.audio_sample_accumulator = 0;
            runtime.audio_sample_area = [0.0; 2];
            runtime.audio_buffer.clear();
            runtime.reset_audio_filter();
            let ticks =
                (WARMUP + FRAMES) as u64 * runtime.tick_hz / u64::from(AUDIO_SAMPLE_RATE_HZ) + 1;
            for tick in 0..ticks {
                let phase = ((tick * u64::from(frequency)) % runtime.tick_hz) as f64
                    / runtime.tick_hz as f64
                    + initial_phase;
                let signal = (TAU * phase).sin() as f32;
                runtime
                    .machine
                    .paula_mut()
                    .set_audio_channel_gain(PaulaChannel::Channel0, signal.max(0.0));
                runtime
                    .machine
                    .paula_mut()
                    .set_audio_channel_gain(PaulaChannel::Channel3, (-signal).max(0.0));
                runtime.sample_audio_after_tick();
            }
            assert_eq!(runtime.audio_buffer.len(), 2 * (WARMUP + FRAMES));
            let ratio = amplitude(&runtime.audio_buffer[WARMUP * 2..], folded) / input_amplitude;
            // A proposed engineering acceptance limit, not a hardware fact:
            // reject stopband aliases by at least 60 dB relative to input.
            let failed = frequency > 24_000 && ratio > 0.001;
            if frequency == 1000 {
                assert!(ratio > 0.9, "silent/broken fixture cannot pass");
            }
            println!(
                "bandwidth,{:?},{frequency},{initial_phase},{folded},{ratio:.9},{}",
                runtime.model(),
                u8::from(failed),
            );
            failures += usize::from(failed);
            count += 1;
        }
    }
    (count, failures)
}

#[test]
#[ignore = "known bandwidth gap; explicit alias-rejection research diagnostic"]
fn host_decimation_rejects_ultrasonic_aliases() {
    let mut count = 0;
    let mut failures = 0;
    for (cases, failed) in [
        sweep(AmigaRuntime::<AmigaOcs>::blank(Model::A500OcsPal)),
        sweep(AmigaRuntime::<AmigaOcs>::blank(Model::A500OcsNtsc)),
        sweep(AmigaRuntime::<AmigaEcs>::blank(Model::A500PlusEcsPal)),
        sweep(AmigaRuntime::<AmigaEcs>::blank(Model::A500PlusEcsNtsc)),
        sweep(AmigaRuntime::<AmigaA1200>::blank(Model::A1200AgaPal)),
        sweep(AmigaRuntime::<AmigaA1200>::blank(Model::A1200AgaNtsc)),
    ] {
        count += cases;
        failures += failed;
    }
    assert_eq!(count, 108);
    assert_eq!(failures, 0, "stopband failures across {count} cases");
}
