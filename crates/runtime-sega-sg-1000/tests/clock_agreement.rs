//! Host clock and captured audio must describe the same elapsed time.
use emu198x_shell::{AudioCapture, HostIo, MachineCore, MachineTime, NullFrameSink, NullTraceSink};
use runtime_sega_sg_1000::{Model, Sg1000Runtime};
#[test]
fn declared_clock_and_audio_rate_agree_for_every_region() {
    for model in [Model::Sg1000Ntsc, Model::Sg1000Pal] {
        // Project-written cartridge: set PSG tone/volume, then loop forever.
        let cart = vec![
            0xf3, 0x3e, 0x80, 0xd3, 0x7f, 0x3e, 0x10, 0xd3, 0x7f, 0x3e, 0x90, 0xd3, 0x7f, 0xc3,
            0x0d, 0,
        ];
        let mut runtime = Sg1000Runtime::new(model, cart);
        let rate = runtime.profile().clock.rate;
        for second in 1..=3 {
            let before = runtime.time().get();
            let mut audio = AudioCapture::default();
            runtime
                .run_until(
                    MachineTime::new(second * rate.numerator_hz / rate.denominator_hz),
                    &mut HostIo {
                        input_events: &[],
                        frame_sink: &mut NullFrameSink,
                        audio_sink: &mut audio,
                        trace_sink: &mut NullTraceSink,
                    },
                )
                .expect("one runtime second");
            let captured = audio.audio().expect("audio");
            let frames = captured.samples.len() as u64 / u64::from(captured.channels);
            let expected =
                (runtime.time().get() - before) * 48_000 * rate.denominator_hz / rate.numerator_hz;
            eprintln!(
                "{model:?}: clock ticks={}, frames={frames}, expected={expected}",
                runtime.time().get()
            );
            assert!(
                captured.samples.iter().any(|v| v.abs() > 0.001),
                "tone must reach output"
            );
            assert!(
                frames.abs_diff(expected) <= 2,
                "{model:?}: {frames} audio frames disagree with {expected} frames of declared elapsed time"
            );
        }
    }
}
