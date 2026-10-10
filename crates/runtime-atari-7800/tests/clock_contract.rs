use emu198x_shell::{
    FamilyRuntime, HostIo, MachineCore, MachineTime, NullAudioSink, NullFrameSink, NullTraceSink,
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
