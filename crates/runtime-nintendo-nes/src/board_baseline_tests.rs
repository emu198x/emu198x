//! Complete pre-split state and continued output from project-written software.
use crate::{InputDevices, Model, NesRuntime};
use emu198x_shell::{
    AudioCapture, HostIo, MachineCore, MachineTime, MediaImage, MediaKind, MediaSet, NullFrameSink,
    NullTraceSink,
};
use std::path::{Path, PathBuf};

fn cartridge() -> Vec<u8> {
    let mut rom = std::fs::read(
        Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../../test-data/synthetic-cartridges/nintendo-nes-logo.nes"),
    )
    .expect("project-written cartridge");
    rom[6] |= 2; // Exercise battery-backed PRG RAM restoration too.
    let vector = 16 + usize::from(rom[4]) * 16_384 - 4;
    let reset = [rom[vector], rom[vector + 1]];
    // Initialise a sustained pulse through CPU instructions, then enter the
    // original plate program. Its absolute addresses remain unchanged.
    let init = [
        0xa9, 1, 0x8d, 0x15, 0x40, 0xa9, 0xbf, 0x8d, 0, 0x40, 0xa9, 0x40, 0x8d, 2, 0x40, 0xa9, 8,
        0x8d, 3, 0x40, 0x4c, reset[0], reset[1],
    ];
    assert!(
        rom[16 + 0x3f00..16 + 0x3f00 + init.len()]
            .iter()
            .all(|b| *b == 0)
    );
    rom[16 + 0x3f00..16 + 0x3f00 + init.len()].copy_from_slice(&init);
    rom[vector..vector + 2].copy_from_slice(&[0, 0xbf]);
    rom
}
fn frame(r: &mut NesRuntime) -> Vec<u32> {
    let mut audio = AudioCapture::default();
    r.run_frame(&mut HostIo {
        input_events: &[],
        frame_sink: &mut NullFrameSink,
        audio_sink: &mut audio,
        trace_sink: &mut NullTraceSink,
    })
    .expect("native field");
    audio
        .audio()
        .expect("audio packet")
        .samples
        .iter()
        .map(|s| s.to_bits())
        .collect()
}
fn specimen(model: Model, devices: InputDevices) -> NesRuntime {
    let mut r = NesRuntime::blank(model);
    let rom = cartridge();
    let mut media = MediaSet::new();
    media.push(MediaImage::new("cartridge-1", MediaKind::Cartridge, &rom));
    r.load_media(&media).expect("load cartridge");
    r.set_input_devices(devices);
    for _ in 0..8 {
        frame(&mut r);
    }
    let m = r.machine_mut().expect("loaded");
    m.set_controller1(0x91);
    m.set_controller2(0x42);
    m.set_controller3(0x24);
    m.set_controller4(0x18);
    m.set_zapper_aim(Some((127, 119)));
    m.set_zapper_trigger(true);
    m.poke(0x125, 0x5a);
    m.poke(0x6125, 0xa5);
    for _ in 0..137 {
        m.tick();
    }
    m.take_audio_buffer();
    let now = m.master_clock();
    r.set_time(MachineTime::new(now));
    r
}
fn output(r: &mut NesRuntime) -> Vec<u8> {
    let mut fields = Vec::new();
    for _ in 0..3 {
        let start = r.time().get();
        let samples = frame(r);
        let m = r.machine().expect("loaded");
        assert!(
            samples.iter().any(|b| f32::from_bits(*b).abs() > 0.0001),
            "guest pulse must sound"
        );
        assert!(
            m.framebuffer().iter().any(|p| *p != m.framebuffer()[0]),
            "guest must paint"
        );
        fields.push((
            r.time().get() - start,
            r.time().get(),
            m.framebuffer_width(),
            m.framebuffer_height(),
            m.framebuffer().to_vec(),
            samples,
        ));
    }
    postcard::to_allocvec(&(fields, r.snapshot().expect("final save"))).expect("output bytes")
}
#[test]
fn regional_boards_preserve_complete_saves_and_continued_output() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../test-data/nes/board-split-v3");
    for model in Model::ALL {
        for (name, devices) in [
            ("pads", InputDevices::Controllers),
            ("four-score", InputDevices::FourScore),
            ("zapper", InputDevices::Zapper),
        ] {
            let name = format!("{}-{name}", model.model_id());
            let mut live = specimen(model, devices);
            let saved = live.snapshot().expect("save");
            let expected = output(&mut live);
            // Preserve original execution independently of the corrected wire layout.
            type Fields = Vec<(u64, u64, u32, u32, Vec<u32>, Vec<u32>)>;
            let legacy = Path::new(env!("CARGO_MANIFEST_DIR"))
                .join(format!("../../test-data/nes/board-split/{name}.output"));
            let old: (Fields, Vec<u8>) =
                postcard::from_bytes(&std::fs::read(legacy).expect("v2 control"))
                    .expect("v2 packets");
            let current: (Fields, Vec<u8>) = postcard::from_bytes(&expected).expect("v3 packets");
            assert!(
                current.0 == old.0,
                "{name}: original pixels, time and audio must not change"
            );
            if let Some(dir) = std::env::var_os("EMU198X_NES_CAPTURE") {
                let dir = PathBuf::from(dir);
                std::fs::create_dir_all(&dir).expect("capture directory");
                for (suffix, bytes) in [("state", &saved), ("output", &expected)] {
                    use std::io::Write;
                    std::fs::OpenOptions::new()
                        .create_new(true)
                        .write(true)
                        .open(dir.join(format!("{name}.{suffix}")))
                        .expect("new captures only")
                        .write_all(bytes)
                        .expect("capture");
                }
                continue;
            }
            let original =
                std::fs::read(root.join(format!("{name}.state"))).expect("original save");
            assert!(saved == original, "{name}: complete original save");
            let reference = std::fs::read(root.join(format!("{name}.output")))
                .expect("original continued output");
            assert!(expected == reference, "{name}: direct output");
            let mut restored = NesRuntime::blank(model);
            restored.restore(&original).expect("restore");
            assert!(
                restored.snapshot().expect("reencoded") == original,
                "{name}: reencoded save"
            );
            assert_eq!(restored.input_devices(), devices);
            assert!(restored.has_battery_backed_ram());
            assert!(
                output(&mut restored) == reference,
                "{name}: first-frame pixels, samples, clocks and final save"
            );
        }
    }
}

#[test]
fn partial_visible_field_restore_keeps_ntsc_waveform_codes() {
    let mut live = specimen(Model::NesNtsc, InputDevices::Controllers);
    for _ in 0..20_000 {
        live.machine_mut().expect("loaded").tick();
    }
    let saved = live.snapshot().expect("save visible field");
    let mut restored = NesRuntime::blank(Model::NesNtsc);
    restored.restore(&saved).expect("restore");
    fn packet(r: &mut NesRuntime) -> emu198x_shell::CapturedFrame {
        let mut frames = emu198x_shell::LatestFrameCapture::default();
        let mut audio = emu198x_shell::NullAudioSink;
        let result = r
            .run_frame(&mut HostIo {
                input_events: &[],
                frame_sink: &mut frames,
                audio_sink: &mut audio,
                trace_sink: &mut NullTraceSink,
            })
            .expect("field");
        assert_eq!(result.stop_reason, emu198x_shell::StopReason::ReachedTarget);
        assert_eq!(frames.frames_seen(), 1);
        frames.frame().expect("emitted packet").clone()
    }
    let direct = packet(&mut live);
    let resumed = packet(&mut restored);
    assert!(
        direct
            .signal
            .as_ref()
            .expect("NTSC source")
            .codes
            .iter()
            .any(|x| *x != 0)
    );
    assert_eq!(
        direct, resumed,
        "the complete first emitted field must match, including waveform codes and phase"
    );
}
