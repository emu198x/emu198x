//! Logical held inputs must survive a live save, including cancelling directions.
use emu198x_shell::{
    AudioCapture, HostIo, InputEvent, LatestFrameCapture, MachineCore, NullTraceSink,
};
use runtime_mattel_aquarius::{AquariusRuntime, Model};

type NativeOutput = (u64, u32, u32, Vec<u8>, Vec<u32>);
fn run(runtime: &mut AquariusRuntime, events: &[InputEvent]) -> NativeOutput {
    let mut frames = LatestFrameCapture::default();
    let mut audio = AudioCapture::default();
    runtime
        .run_frame(&mut HostIo {
            input_events: events,
            frame_sink: &mut frames,
            audio_sink: &mut audio,
            trace_sink: &mut NullTraceSink,
        })
        .expect("native frame");
    assert_eq!(frames.frames_seen(), 1);
    let f = frames.frame().expect("native output");
    (
        f.timestamp.get(),
        f.width,
        f.height,
        f.pixels.clone(),
        audio
            .audio()
            .expect("audio")
            .samples
            .iter()
            .map(|s| s.to_bits())
            .collect(),
    )
}

fn button(port: u8, name: &'static str, pressed: bool) -> InputEvent {
    InputEvent::Button {
        port,
        name: name.into(),
        pressed,
    }
}
fn runtime(model: Model) -> AquariusRuntime {
    let mut rom = vec![0; 8192];
    rom[..2].copy_from_slice(&[0x18, 0xfe]);
    AquariusRuntime::new(model, rom).expect("synthetic ROM")
}

#[test]
fn release_after_restore_matches_uninterrupted_controller_history() {
    let mut failures = Vec::new();
    let mut checked = 0;
    for model in Model::ALL {
        for port in [1, 2] {
            for (first, second, release) in [
                ("up", "fire", "fire"),
                ("up", "fire", "up"),
                ("up", "down", "down"),
                ("left", "right", "right"),
            ] {
                for dirty_target in [false, true] {
                    let mut original = runtime(model);
                    run(
                        &mut original,
                        &[button(port, first, true), button(port, second, true)],
                    );
                    let state = original.snapshot().expect("snapshot");
                    let mut restored = if dirty_target {
                        runtime(model)
                    } else {
                        AquariusRuntime::blank(model)
                    };
                    if dirty_target {
                        run(
                            &mut restored,
                            &[button(port, "left", true), button(port, "fire", true)],
                        );
                    }
                    restored.restore(&state).expect("restore");
                    assert_eq!(restored.snapshot().expect("round trip"), state);
                    let event = button(port, release, false);
                    let original_output = run(&mut original, std::slice::from_ref(&event));
                    let restored_output = run(&mut restored, std::slice::from_ref(&event));
                    assert!(
                        original_output == restored_output,
                        "{model:?} port {port}: complete native output"
                    );
                    let expected = original
                        .machine()
                        .expect("original board")
                        .controller_bytes();
                    let actual = restored
                        .machine()
                        .expect("restored board")
                        .controller_bytes();
                    if actual != expected
                        || restored.snapshot().expect("resumed state")
                            != original.snapshot().expect("original state")
                    {
                        failures.push(format!("{model:?} port {port} {first}+{second} release {release} dirty={dirty_target}: original={expected:02x?} restored={actual:02x?}"));
                    }
                    checked += 1;
                }
            }
        }
    }
    assert_eq!(checked, 32);
    assert!(failures.is_empty(), "{}", failures.join("\n"));
}

#[test]
fn complete_original_v4_saves_reject_before_decoding_without_mutation() {
    let originals: &[&[u8]] = &[
        include_bytes!("fixtures/controller-v4/mattel-aquarius-p1-left-right.state").as_slice(),
        include_bytes!("fixtures/controller-v4/mattel-aquarius-p1-up-down.state").as_slice(),
        include_bytes!("fixtures/controller-v4/mattel-aquarius-p1-up-fire.state").as_slice(),
        include_bytes!("fixtures/controller-v4/mattel-aquarius-p2-left-right.state").as_slice(),
        include_bytes!("fixtures/controller-v4/mattel-aquarius-p2-up-down.state").as_slice(),
        include_bytes!("fixtures/controller-v4/mattel-aquarius-p2-up-fire.state").as_slice(),
        include_bytes!("fixtures/controller-v4/mattel-aquarius-pal-p1-left-right.state").as_slice(),
        include_bytes!("fixtures/controller-v4/mattel-aquarius-pal-p1-up-down.state").as_slice(),
        include_bytes!("fixtures/controller-v4/mattel-aquarius-pal-p1-up-fire.state").as_slice(),
        include_bytes!("fixtures/controller-v4/mattel-aquarius-pal-p2-left-right.state").as_slice(),
        include_bytes!("fixtures/controller-v4/mattel-aquarius-pal-p2-up-down.state").as_slice(),
        include_bytes!("fixtures/controller-v4/mattel-aquarius-pal-p2-up-fire.state").as_slice(),
    ];
    assert_eq!(originals.len(), 12);
    let mut checked = 0;
    for model in Model::ALL {
        let mut target = runtime(model);
        run(
            &mut target,
            &[button(2, "left", true), button(1, "fire", true)],
        );
        let before = target.snapshot().expect("current state");
        for bytes in originals
            .iter()
            .copied()
            .chain(std::iter::once([4_u8].as_slice()))
        {
            let error = target
                .restore(bytes)
                .expect_err("v4 omits logical input history");
            assert!(
                matches!(error, emu198x_shell::MachineError::InvalidSnapshot { ref reason }
                if reason == "unsupported snapshot version 4; expected 5"),
                "{error}"
            );
            assert_eq!(target.snapshot().expect("atomic rejection"), before);
            checked += 1;
        }
    }
    assert_eq!(checked, 26);
}
