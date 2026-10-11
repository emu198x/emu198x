use emu198x_shell::{
    AudioPacket, AudioSink, FamilyRuntime, HostIo, MachineCore, MachineError, MachineTime,
    NullAudioSink, NullFrameSink, NullTraceSink,
};
use runtime_atari_7800::{Atari7800Runtime, Model};

fn cartridge() -> Vec<u8> {
    let mut rom = vec![0xea; 32768];
    rom[..3].copy_from_slice(&[0x4c, 0x00, 0x80]);
    for vector in [0x7ffa, 0x7ffc, 0x7ffe] {
        rom[vector..vector + 2].copy_from_slice(&[0x00, 0x80]);
    }
    rom
}

#[test]
fn host_budget_matches_live_machine_frames() {
    for model in Model::ALL {
        let mut runtime = Atari7800Runtime::new(model, cartridge()).expect("cartridge");
        let budget = runtime.native_frame_ticks();
        let (mut frames, mut audio, mut trace) = (NullFrameSink, NullAudioSink, NullTraceSink);
        let mut host = HostIo {
            input_events: &[],
            frame_sink: &mut frames,
            audio_sink: &mut audio,
            trace_sink: &mut trace,
        };
        for count in 1..=3 {
            runtime
                .run_until(MachineTime::new(budget * count), &mut host)
                .expect("run");
            let machine = runtime.machine().expect("machine");
            assert_eq!(machine.frame_count(), count);
            assert_eq!(machine.master_clock(), budget * count, "{model:?}");
            assert_eq!(runtime.time().get(), machine.master_clock());
        }
    }
}

#[test]
fn profile_rate_uses_the_regional_native_oscillator() {
    for (model, hz) in [
        (Model::A7800Ntsc, 14_318_180),
        (Model::A7800Pal, 14_187_576),
    ] {
        let runtime = Atari7800Runtime::blank(model);
        let clock = &runtime.profile().clock;
        assert_eq!(clock.rate.numerator_hz, hz, "{model:?}");
        assert_eq!(clock.rate.denominator_hz, 1);
        assert_eq!(clock.unit, "master-clock");
    }
}

#[test]
fn version_five_is_rejected_without_mutating_live_state() {
    let mut runtime = Atari7800Runtime::new(Model::A7800Ntsc, cartridge()).expect("cartridge");
    runtime.machine_mut().expect("machine").run_frame();
    let before = runtime.snapshot().expect("snapshot");
    let mut old = before.clone();
    old[0] = 5;
    let error = runtime
        .restore(&old)
        .expect_err("colour-clock saves must be rejected");
    assert!(
        error.to_string().contains("unsupported snapshot version 5"),
        "{error}"
    );
    assert_eq!(runtime.snapshot().expect("unchanged snapshot"), before);
}

#[test]
fn emitted_audio_rate_matches_native_sample_production() {
    #[derive(Default)]
    struct AudioCapture(Vec<(u64, u32, usize)>);
    impl AudioSink for AudioCapture {
        fn push_audio(&mut self, packet: AudioPacket<'_>) -> Result<(), MachineError> {
            assert_eq!(packet.channels, 1);
            self.0.push((
                packet.timestamp.get(),
                packet.sample_rate,
                packet.samples.len(),
            ));
            Ok(())
        }
    }
    for model in Model::ALL {
        let mut runtime = Atari7800Runtime::new(model, cartridge()).expect("cartridge");
        let budget = runtime.native_frame_ticks();
        let hz = runtime.machine().expect("machine").region().master_hz();
        let (mut frames, mut audio, mut trace) =
            (NullFrameSink, AudioCapture::default(), NullTraceSink);
        let mut host = HostIo {
            input_events: &[],
            frame_sink: &mut frames,
            audio_sink: &mut audio,
            trace_sink: &mut trace,
        };
        runtime
            .run_until(MachineTime::new(budget * 6), &mut host)
            .expect("run");
        assert_eq!(audio.0.len(), 6);
        let rate = audio.0[0].1;
        let mut samples = 0;
        for (index, &(timestamp, packet_rate, count)) in audio.0.iter().enumerate() {
            assert_eq!(timestamp, budget * (index as u64 + 1));
            assert_eq!(packet_rate, rate);
            assert!(count > 0);
            samples += count as u64;
        }
        assert!(
            (samples * hz).abs_diff(u64::from(rate) * budget * 6) < hz,
            "host packet rate differs from native sample production for {model:?}"
        );
    }
}
