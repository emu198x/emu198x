//! Project-written cartridge and complete pre-correction v5 continuations.
use crate::{Model, Sg1000Runtime};
use emu198x_shell::{
    AudioCapture, HostIo, LatestFrameCapture, MachineCore, MachineTime, NullTraceSink, StopReason,
};
use emu198x_zilog_z80::Z80Stepper;
use std::path::Path;

fn cartridge() -> Vec<u8> {
    let mut rom = vec![0; 8192];
    rom[..3].copy_from_slice(&[0xc3, 0, 1]);
    rom[0x66..0x6e].copy_from_slice(&[0x3e, 0xa7, 0x32, 3, 0xc0, 0xed, 0x45, 0]);
    let mut code = vec![0xf3, 0x31, 0xff, 0xc3];
    fn out(code: &mut Vec<u8>, port: u8, value: u8) {
        code.extend([0x3e, value, 0xd3, port]);
    }
    for (register, value) in [(1, 0x40), (2, 0x0e), (3, 0x80), (4, 0), (7, 4)] {
        out(&mut code, 0xbf, value);
        out(&mut code, 0xbf, 0x80 | register);
    }
    out(&mut code, 0xbf, 0);
    out(&mut code, 0xbf, 0x40);
    for _ in 0..8 {
        out(&mut code, 0xbe, 0xff);
    }
    out(&mut code, 0xbf, 0);
    out(&mut code, 0xbf, 0x60);
    out(&mut code, 0xbe, 0xf2);
    for value in [0x80, 0x10, 0x90] {
        out(&mut code, 0x7f, value);
    }
    let entry = 0x100 + code.len() as u16;
    code.extend([
        0x21,
        0,
        0xc0,
        0x34,
        0xdb,
        0xdd,
        0x32,
        1,
        0xc0,
        0xdb,
        0xdc,
        0x32,
        2,
        0xc0,
        0xc3,
        entry as u8,
        (entry >> 8) as u8,
    ]);
    rom[0x100..0x100 + code.len()].copy_from_slice(&code);
    rom
}
fn field(runtime: &mut Sg1000Runtime) -> (u64, emu198x_shell::CapturedFrame, Vec<u32>) {
    let before = runtime.time().get();
    let mut video = LatestFrameCapture::default();
    let mut audio = AudioCapture::default();
    let result = runtime
        .run_frame(&mut HostIo {
            input_events: &[],
            frame_sink: &mut video,
            audio_sink: &mut audio,
            trace_sink: &mut NullTraceSink,
        })
        .expect("native field");
    assert_eq!(result.stop_reason, StopReason::ReachedTarget);
    assert_eq!(video.frames_seen(), 1);
    let picture = video.frame().expect("picture").clone();
    assert!(
        picture
            .pixels
            .as_chunks::<4>()
            .0
            .iter()
            .any(|pixel| pixel.as_slice() != &picture.pixels[..4]),
        "guest video must be active"
    );
    let sound = &audio.audio().expect("audio").samples;
    assert!(
        sound.iter().any(|v| v.abs() > 0.001),
        "guest tone must reach output"
    );
    (
        runtime.time().get() - before,
        picture,
        sound.iter().map(|v| v.to_bits()).collect(),
    )
}
fn specimen(model: Model) -> Sg1000Runtime {
    let mut runtime = Sg1000Runtime::new(model, cartridge());
    for _ in 0..3 {
        field(&mut runtime);
    }
    let machine = runtime.machine_mut().expect("machine");
    machine.controller1_mut().button1 = true;
    machine.controller2_mut().right = true;
    machine.controller2_mut().button2 = true;
    machine.set_pause_pressed(true);
    machine.poke(0xc007, 0x5a);
    for _ in 0..137 {
        machine.step_tick();
    }
    assert_eq!(machine.peek(0xc003), 0xa7, "hold NMI handler ran");
    let time = machine.cpu_tstates();
    runtime.set_time(MachineTime::new(time));
    runtime
}
// Test-only original envelope encoder; production always writes v6.
fn legacy_snapshot(runtime: &Sg1000Runtime) -> Vec<u8> {
    postcard::to_allocvec(&(
        5_u16,
        runtime.time().get(),
        runtime.model().model_id(),
        runtime.machine().map(|m| m.state()),
    ))
    .expect("legacy envelope")
}
fn continuation(runtime: &mut Sg1000Runtime) -> Vec<u8> {
    let fields: Vec<_> = (0..3).map(|_| field(runtime)).collect();
    postcard::to_allocvec(&(fields, legacy_snapshot(runtime))).expect("output")
}
#[test]
fn original_v5_states_and_native_continuations_are_exact() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../test-data/sega-sg-1000/board-v5");
    for model in Model::ALL {
        if model == Model::Sg1000Pal {
            let original = std::fs::read(root.join(format!("{}.state", model.model_id())))
                .expect("original PAL");
            let mut live = specimen(model);
            let before = live.snapshot().expect("before");
            assert!(
                live.restore(&original)
                    .expect_err("legacy PAL rejected")
                    .to_string()
                    .contains("v5 PAL")
            );
            assert!(live.snapshot().expect("unchanged") == before);
            continue;
        }
        let mut live = specimen(model);
        let saved = legacy_snapshot(&live);
        let output = continuation(&mut live);
        if let Some(dir) = std::env::var_os("EMU198X_SG1000_CAPTURE") {
            use std::io::Write;
            let dir = std::path::PathBuf::from(dir);
            std::fs::create_dir_all(&dir).expect("capture dir");
            for (suffix, bytes) in [("state", &saved), ("output", &output)] {
                std::fs::OpenOptions::new()
                    .write(true)
                    .create_new(true)
                    .open(dir.join(format!("{}.{}", model.model_id(), suffix)))
                    .expect("new capture only")
                    .write_all(bytes)
                    .expect("capture");
            }
            continue;
        }
        let original = std::fs::read(root.join(format!("{}.state", model.model_id())))
            .expect("original state");
        let expected = std::fs::read(root.join(format!("{}.output", model.model_id())))
            .expect("original output");
        assert!(saved == original, "original state bytes");
        assert!(output == expected, "direct continuation");
        let mut restored = Sg1000Runtime::blank(model);
        restored.restore(&original).expect("restore");
        assert!(legacy_snapshot(&restored) == original, "original envelope");
        assert!(
            continuation(&mut restored) == expected,
            "first resumed pixels/audio/clocks/final state"
        );
    }
}

#[test]
fn corrected_pal_v6_retains_partial_field_continuation() {
    let mut live = specimen(Model::Sg1000Pal);
    let saved = live.snapshot().expect("v6");
    assert_eq!(
        postcard::take_from_bytes::<u16>(&saved).expect("version").0,
        6
    );
    let mut restored = Sg1000Runtime::blank(Model::Sg1000Pal);
    restored.restore(&saved).expect("restore v6");
    assert!(restored.snapshot().expect("same envelope") == saved);
    assert!(
        continuation(&mut live) == continuation(&mut restored),
        "first pixels/audio/clocks/final state"
    );
}
