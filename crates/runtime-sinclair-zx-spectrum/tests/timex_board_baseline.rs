//! Original pre-split v17 envelopes and complete continued output.
use emu198x_shell::{
    AudioCapture, FamilyRuntime, FirmwareImage, FirmwareSet, HostIo, InputEvent,
    LatestFrameCapture, MachineCore, NullTraceSink,
};
use runtime_sinclair_zx_spectrum::{Model, SpectrumLiveAccess, SpectrumRuntimeKind};
use std::path::{Path, PathBuf};

fn runtime(model: Model) -> SpectrumRuntimeKind {
    let mut rom = [0; 16384];
    // DI; paint a pixel/attribute; increment HOME RAM forever.
    let code = [
        0xf3, 0x3e, 0xff, 0x32, 0, 0x40, 0x3e, 0x47, 0x32, 0, 0x58, 0x21, 0, 0x80, 0x34, 0x18, 0xfd,
    ];
    rom[..code.len()].copy_from_slice(&code);
    let exrom = [0x42; 8192];
    let mut fw = FirmwareSet::new();
    fw.push(FirmwareImage::new("timex-ts2068-rom-0", &rom));
    fw.push(FirmwareImage::new("timex-ts2068-rom-1", &exrom));
    SpectrumRuntimeKind::from_firmware(model, &fw).expect("synthetic firmware")
}
type FrameOutput = (u64, u32, u32, Vec<u8>, Option<Vec<u32>>, Vec<u32>);
fn frame(r: &mut SpectrumRuntimeKind) -> FrameOutput {
    let mut f = LatestFrameCapture::default();
    let mut a = AudioCapture::default();
    let mut host = HostIo {
        input_events: &[],
        frame_sink: &mut f,
        audio_sink: &mut a,
        trace_sink: &mut NullTraceSink,
    };
    r.run_frame(&mut host).expect("native boundary");
    assert_eq!(f.frames_seen(), 1);
    let f = f.frame().expect("frame");
    (
        f.timestamp.get(),
        f.width,
        f.height,
        f.pixels.clone(),
        f.palette.clone(),
        a.audio()
            .expect("audio")
            .samples
            .iter()
            .map(|s| s.to_bits())
            .collect(),
    )
}
fn specimen(model: Model, mode: u8) -> SpectrumRuntimeKind {
    let mut r = runtime(model);
    for (reg, value) in [(0, 24), (1, 0), (7, 0x3e), (8, 15)] {
        r.port_write(0xf5, reg);
        r.port_write(0xf6, value);
    }
    r.port_write(0xfe, 0x12);
    r.port_write(0xff, mode);
    r.port_write(0xf4, 0x08); // Empty DOCK at $6000, HOME code remains executable.
    r.write_byte(0x9000, 0x5a);
    for _ in 0..2 {
        frame(&mut r);
    }
    let time = r.time();
    r.run_until(
        time,
        &mut HostIo {
            input_events: &[InputEvent::Key {
                name: "a".into(),
                pressed: true,
            }],
            frame_sink: &mut emu198x_shell::NullFrameSink,
            audio_sink: &mut emu198x_shell::NullAudioSink,
            trace_sink: &mut NullTraceSink,
        },
    )
    .expect("input without advancing");
    assert_eq!(r.profile_cycles(137).expect("partial field").ticks, 137);
    assert_eq!(r.time().get(), time.get() + 137);
    assert_eq!(r.read_byte(0x9000), 0x5a);
    r
}
fn output(r: &mut SpectrumRuntimeKind) -> Vec<u8> {
    let fields: Vec<_> = (0..3).map(|_| frame(r)).collect();
    assert!(
        fields.iter().flat_map(|f| &f.5).any(|s| *s != 0),
        "live audio"
    );
    postcard::to_allocvec(&(fields, r.snapshot().expect("final state"))).expect("output")
}
fn root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../../test-data/timex-2068/board-split-v17")
}
#[test]
fn original_saves_keep_pixels_audio_time_and_paging_on_both_boards() {
    std::thread::Builder::new()
        .stack_size(16 * 1024 * 1024)
        .spawn(check_original_saves)
        .expect("test thread")
        .join()
        .expect("test result");
}
fn check_original_saves() {
    for model in [Model::TimexTC2068, Model::TimexTS2068] {
        for mode in [0, 6] {
            let name = format!("{}-mode{mode}", model.model_id());
            let mut r = specimen(model, mode);
            let state = r.snapshot().expect("state");
            let out = output(&mut r);
            if let Some(dir) = std::env::var_os("EMU198X_TIMEX_CAPTURE") {
                let dir = PathBuf::from(dir);
                std::fs::create_dir_all(&dir).expect("capture dir");
                for (ext, bytes) in [("state", &state), ("output", &out)] {
                    use std::io::Write;
                    std::fs::OpenOptions::new()
                        .write(true)
                        .create_new(true)
                        .open(dir.join(format!("{name}.{ext}")))
                        .expect("new capture only")
                        .write_all(bytes)
                        .expect("capture");
                }
                continue;
            }
            let saved =
                std::fs::read(root().join(format!("{name}.state"))).expect("original state");
            let expected =
                std::fs::read(root().join(format!("{name}.output"))).expect("original output");
            assert!(state == saved, "{name}: original complete v17 save");
            assert!(out == expected, "{name}: complete continued output");
            let mut restored = runtime(model);
            restored.restore(&saved).expect("original restore");
            assert!(
                restored.snapshot().expect("reencoded") == saved,
                "{name}: unchanged save bytes"
            );
            assert!(
                output(&mut restored) == expected,
                "{name}: restored continuation"
            );
        }
    }
}
