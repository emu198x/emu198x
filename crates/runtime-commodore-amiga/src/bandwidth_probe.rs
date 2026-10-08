//! Regression for host-decimation bandwidth, with the original failing inventory.
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
    for frequency in [
        1000_u32, 10_000, 20_000, 24_000, 28_000, 55_000, 95_000, 150_000, 500_000, 1_000_000,
        3_000_000,
    ] {
        let folded = frequency % AUDIO_SAMPLE_RATE_HZ;
        let folded = folded.min(AUDIO_SAMPLE_RATE_HZ - folded);
        for initial_phase in [0.0, 0.37, 0.91] {
            runtime.audio_sample_accumulator = 0;
            runtime.audio_resampler = BandLimitedStereo::default();
            runtime.audio_led_history = 0;
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
            let failed = if frequency >= 24_000 {
                ratio > 0.001
            } else {
                // The resampler should retain the board filter's in-band
                // response. Compare with that filter at the host rate.
                let mut filter = crate::audio_filter::AmigaAudioFilter::for_model(runtime.model());
                let mut expected = Vec::new();
                for n in 0..WARMUP + FRAMES {
                    let mut left = 0.0;
                    let mut right = (TAU * f64::from(frequency) * n as f64 / 48_000.0).sin() as f32;
                    filter.apply(&mut left, &mut right, false);
                    expected.extend([left, right]);
                }
                let target = amplitude(&expected[WARMUP * 2..], frequency);
                (ratio / target - 1.0).abs() > 0.001
            };
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
    assert_eq!(count, 198);
    assert_eq!(failures, 0, "bandwidth failures across {count} cases");
}

// Independent continuous impulse response and Simpson integration. This
// oracle never reads the production coefficient table or step-response ring.
fn impulse(t: f64) -> f64 {
    if !(0.0..=96.0).contains(&t) {
        return 0.0;
    }
    let x = t - 48.0;
    let window = 0.42 + 0.5 * (TAU * x / 96.0).cos() + 0.08 * (2.0 * TAU * x / 96.0).cos();
    let sinc = if x.abs() < 1e-12 {
        44.0 / 48.0
    } else {
        (TAU * 22.0 / 48.0 * x).sin() / (std::f64::consts::PI * x)
    };
    window * sinc
}

fn integral(a: f64, b: f64, steps: usize) -> f64 {
    let step = (b - a) / steps as f64;
    let mut sum = impulse(a) + impulse(b);
    for i in 1..steps {
        sum += impulse(a + i as f64 * step) * if i % 2 == 0 { 2.0 } else { 4.0 };
    }
    sum * step / 3.0
}

pub(super) fn pulse_weight(a: f64, b: f64) -> f64 {
    static NORMALIZATION: std::sync::LazyLock<f64> =
        std::sync::LazyLock::new(|| integral(0.0, 96.0, 32768));
    integral(a, b, 64) / *NORMALIZATION
}

fn led_alignment<M: AmigaMachine + AmigaLiveAccess + AmigaDriver>(mut runtime: AmigaRuntime<M>) {
    use std::collections::VecDeque;
    for (offset, value) in [(0xa6, 1000), (0xa8, 64), (0xaa, 0x4040)] {
        AmigaDriver::dispatch_custom_write(runtime.machine_mut(), offset, value);
    }
    runtime.machine.cia_a_mut().write(2, 2);
    let level = runtime.machine.mix_audio_stereo();
    let mut signal = BandLimitedStereo::default();
    signal.observe([f64::from(level.0), f64::from(level.1)], 0, runtime.tick_hz);
    let mut filter = crate::audio_filter::AmigaAudioFilter::for_model(runtime.model());
    // Independent FIFO, deliberately not a copy of the bit-shift register.
    let mut controls = VecDeque::from(vec![false; 48]);
    for frame in 0..160 {
        let bright = frame < 17 || (19..89).contains(&frame);
        runtime
            .machine
            .cia_a_mut()
            .write(0, if bright { 0 } else { 2 });
        let delayed = controls.pop_front().expect("fixed control delay");
        controls.push_back(bright);
        while runtime.audio_buffer.len() <= frame * 2 {
            runtime.sample_audio_after_tick();
        }
        let [left, right] = signal.emit();
        let (mut left, mut right) = (left as f32, right as f32);
        filter.apply(&mut left, &mut right, delayed);
        assert_eq!(runtime.audio_buffer[frame * 2], left.clamp(-1.0, 1.0));
        assert_eq!(
            runtime.audio_buffer[frame * 2 + 1],
            right.clamp(-1.0, 1.0),
            "{:?}, frame {frame}",
            runtime.model()
        );
    }
    assert!(runtime.audio_buffer.iter().any(|v| v.abs() > 0.1));
}

#[test]
fn led_control_follows_the_delayed_signal_on_every_board() {
    led_alignment(AmigaRuntime::<AmigaOcs>::blank(Model::A500OcsPal));
    led_alignment(AmigaRuntime::<AmigaOcs>::blank(Model::A500OcsNtsc));
    led_alignment(AmigaRuntime::<AmigaOcs>::blank(Model::A1000OcsPal));
    led_alignment(AmigaRuntime::<AmigaOcs>::blank(Model::A1000OcsNtsc));
    led_alignment(AmigaRuntime::<AmigaEcs>::blank(Model::A500PlusEcsPal));
    led_alignment(AmigaRuntime::<AmigaEcs>::blank(Model::A500PlusEcsNtsc));
    led_alignment(AmigaRuntime::<AmigaA1200>::blank(Model::A1200AgaPal));
    led_alignment(AmigaRuntime::<AmigaA1200>::blank(Model::A1200AgaNtsc));
}

#[test]
#[ignore = "DIAGNOSTIC: explicit wall-clock benchmark; not a correctness gate"]
fn benchmark_runtime_audio_conversion() {
    use std::hint::black_box;
    use std::time::Instant;
    for band_limited in [false, true] {
        for repeat in 0..3 {
            for edge_ticks in [1_u64, 2, 16, 248] {
                let mut runtime = AmigaRuntime::<AmigaA1200>::blank(Model::A1200AgaPal);
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
                runtime.audio_buffer.reserve(96_000);
                let mut baseline_area = [0.0; 2];
                let start = Instant::now();
                for tick in 0..runtime.tick_hz {
                    let positive = (tick / edge_ticks) % 2 == 0;
                    runtime.machine.paula_mut().set_audio_channel_gain(
                        PaulaChannel::Channel0,
                        if positive { 1.0 } else { 0.0 },
                    );
                    runtime.machine.paula_mut().set_audio_channel_gain(
                        PaulaChannel::Channel3,
                        if positive { 0.0 } else { 1.0 },
                    );
                    if band_limited {
                        runtime.sample_audio_after_tick();
                    } else {
                        // Historical v61 box conversion (461d92f4), retained only
                        // to measure incremental cost with the identical fixture.
                        let rate = u64::from(AUDIO_SAMPLE_RATE_HZ);
                        let remaining = runtime.tick_hz - runtime.audio_sample_accumulator;
                        let weight = remaining.min(rate);
                        let (left, right) = runtime.machine.mix_audio_stereo();
                        let level = [f64::from(left), f64::from(right)];
                        for (area, sample) in baseline_area.iter_mut().zip(level) {
                            *area += sample * weight as f64;
                        }
                        if rate < remaining {
                            runtime.audio_sample_accumulator += rate;
                        } else {
                            let mut left = (baseline_area[0] / runtime.tick_hz as f64) as f32;
                            let mut right = (baseline_area[1] / runtime.tick_hz as f64) as f32;
                            runtime.audio_filter.apply(
                                &mut left,
                                &mut right,
                                runtime.machine.led_filter_engaged(),
                            );
                            runtime
                                .audio_buffer
                                .extend([left.clamp(-1.0, 1.0), right.clamp(-1.0, 1.0)]);
                            runtime.audio_sample_accumulator = rate - weight;
                            baseline_area = level.map(|sample| sample * (rate - weight) as f64);
                        }
                    }
                }
                let seconds = start.elapsed().as_secs_f64();
                assert_eq!(runtime.audio_buffer.len(), 96_000);
                black_box(&runtime.audio_buffer);
                println!(
                    "runtime_audio,band_limited,{band_limited},repeat,{repeat},edge_ticks,{edge_ticks},seconds,{seconds:.6}"
                );
            }
        }
    }
}
