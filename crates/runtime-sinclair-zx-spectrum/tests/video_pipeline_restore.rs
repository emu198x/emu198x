//! Save/restore must preserve observable output from the first resumed tick.
use emu198x_shell::{
    AudioCapture, FamilyRuntime, FirmwareImage, FirmwareSet, HostIo, LatestFrameCapture,
    MachineCore, NullTraceSink,
};
use runtime_sinclair_zx_spectrum::{Model, SpectrumLiveAccess, SpectrumRuntimeKind, profile_for};

fn runtime(model: Model) -> SpectrumRuntimeKind {
    let mut rom = [0; 16384];
    // DI; jump to self. Rendering and sound remain live while the CPU runs.
    rom[..4].copy_from_slice(&[0xf3, 0xc3, 1, 0]);
    let exrom = [0; 8192];
    let profile = profile_for(model);
    let mut firmware = FirmwareSet::new();
    for requirement in &profile.firmware {
        let id = requirement.id.as_ref();
        firmware.push(FirmwareImage::new(
            requirement.id.clone(),
            if id == "timex-ts2068-rom-1" {
                &exrom
            } else {
                &rom
            },
        ));
    }
    SpectrumRuntimeKind::from_firmware(model, &firmware).expect("synthetic firmware")
}
type Output = (u64, u32, u32, Vec<u8>, Option<Vec<u32>>, Vec<u32>);
fn frame(r: &mut SpectrumRuntimeKind) -> Output {
    let mut pixels = LatestFrameCapture::default();
    let mut audio = AudioCapture::default();
    r.run_frame(&mut HostIo {
        input_events: &[],
        frame_sink: &mut pixels,
        audio_sink: &mut audio,
        trace_sink: &mut NullTraceSink,
    })
    .expect("native boundary");
    assert_eq!(pixels.frames_seen(), 1);
    let f = pixels.frame().expect("frame");
    (
        f.timestamp.get(),
        f.width,
        f.height,
        f.pixels.clone(),
        f.palette.clone(),
        audio
            .audio()
            .expect("audio")
            .samples
            .iter()
            .map(|s| s.to_bits())
            .collect(),
    )
}
fn run_on_test_stack(test: impl FnOnce() + Send + 'static) {
    std::thread::Builder::new()
        .stack_size(16 * 1024 * 1024)
        .spawn(test)
        .expect("test thread")
        .join()
        .expect("test result");
}

#[test]
fn every_family_profile_resumes_exact_pixels_audio_clock_and_state() {
    run_on_test_stack(|| {
        let mut cases = 0;
        for model in Model::ALL {
            let timex = matches!(
                model,
                Model::TimexTC2048 | Model::TimexTC2068 | Model::TimexTS2068
            );
            let modes: &[u8] = if timex { &[0, 1, 2, 6] } else { &[0] };
            for &mode in modes {
                for phase in [0, 1, 7, 8, 9, 15, 17, 137] {
                    cases += 1;
                    let mut original = runtime(model);
                    for addr in 0x4000u16..0x8000 {
                        original.write_byte(addr, (addr as u8).wrapping_mul(17) ^ 0x5a);
                    }
                    original.port_write(0xff, mode | 0x28);
                    for (reg, value) in [(0, 24), (1, 0), (7, 0x3e), (8, 15)] {
                        original.port_write(0xf5, reg);
                        original.port_write(0xf6, value);
                        original.port_write(0xfffd, reg);
                        original.port_write(0xbffd, value);
                    }
                    original.port_write(0xfe, 0x12);
                    frame(&mut original);
                    frame(&mut original);
                    if phase > 0 {
                        assert_eq!(
                            original.profile_cycles(phase).expect("partial ticks").ticks,
                            u64::from(phase)
                        );
                    }
                    let saved = original.snapshot().expect("snapshot");
                    let mut restored = runtime(model);
                    restored.restore(&saved).expect("restore");
                    assert!(
                        saved == restored.snapshot().expect("reencoded"),
                        "{model:?} mode {mode} phase {phase}: save bytes"
                    );
                    for field in 0..2 {
                        let a = frame(&mut original);
                        let b = frame(&mut restored);
                        assert!(
                            a == b,
                            "{model:?} mode {mode} phase {phase} field {field}: pixels, palette, clock and sample bits"
                        );
                    }
                    assert!(
                        original.snapshot().expect("uninterrupted")
                            == restored.snapshot().expect("resumed"),
                        "{model:?} mode {mode} phase {phase}: final state"
                    );
                }
            }
        }
        assert_eq!(
            cases, 176,
            "every profile, video mode and phase was exercised"
        );
    });
}

#[test]
fn legacy_v16_is_rejected_before_payload_decode_without_mutation() {
    run_on_test_stack(|| {
        for model in Model::ALL {
            let mut r = runtime(model);
            r.write_byte(0x5000, 0xa5);
            frame(&mut r);
            let before = r.snapshot().expect("before");
            // Deliberately no machine payload: version rejection must come first.
            let error = r.restore(&[16]).expect_err("v16 lacks causal state");
            assert!(
                matches!(error,emu198x_shell::MachineError::InvalidSnapshot {reason} if reason=="unsupported snapshot version 16; expected 17")
            );
            assert!(
                before == r.snapshot().expect("after"),
                "{model:?}: atomic rejection"
            );
        }
    });
}

#[test]
fn original_complete_v16_envelopes_are_rejected_atomically() {
    run_on_test_stack(|| {
        for model in [Model::TimexTC2068, Model::TimexTS2068] {
            for mode in [0, 6] {
                let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join(format!(
                    "../../test-data/timex-2068/legacy-v16/{}-mode{mode}.state",
                    model.model_id()
                ));
                let old = std::fs::read(path).expect("complete original save");
                assert_eq!(old[0], 16);
                let mut r = runtime(model);
                frame(&mut r);
                r.write_byte(0x5000, 0xa5);
                let before = r.snapshot().expect("before");
                assert!(
                    matches!(r.restore(&old),Err(emu198x_shell::MachineError::InvalidSnapshot {reason}) if reason=="unsupported snapshot version 16; expected 17")
                );
                assert!(before == r.snapshot().expect("after"));
            }
        }
    });
}
