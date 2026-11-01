//! Immutable v66 saves and native continuations from before board extraction.
use std::path::{Path, PathBuf};

use emu198x_shell::{
    AudioPacket, AudioSink, HostIo, InputEvent, LatestFrameCapture, MachineCore, MachineError,
    MediaImage, MediaKind, MediaSet, NullAudioSink, NullFrameSink, NullTraceSink, StopReason,
};
use runtime_commodore_amiga::{AmigaLiveAccess, AmigaRuntimeKind, Model};
use serde::Serialize;
use sha2::{Digest, Sha256};

const ORIGINAL_HEAD: &str = "cbcda19a116bdf3a86a8d41c186c046842c342b8";

fn firmware(active: bool, interlace: bool) -> Vec<u8> {
    let mut rom = vec![0; 512 * 1024];
    rom[..4].copy_from_slice(&0x001F_F000u32.to_be_bytes());
    rom[4..8].copy_from_slice(&0x00F8_0008u32.to_be_bytes());
    if !active {
        rom[8..10].copy_from_slice(&[0x60, 0xFE]);
        return rom;
    }
    let mut code = Vec::new();
    fn immediate(code: &mut Vec<u8>, opcode: u16, value: u16, address: u32) {
        code.extend(opcode.to_be_bytes());
        code.extend(value.to_be_bytes());
        code.extend(address.to_be_bytes());
    }
    // MOVE.B #imm,abs.l: enable CIA-A OVL/LED outputs, then remove overlay.
    immediate(&mut code, 0x13FC, 3, 0x00BF_E201);
    immediate(&mut code, 0x13FC, 2, 0x00BF_E001);
    for (address, value) in [
        (0x0000_1000, 0x7F80),
        (0x0000_1002, 0x4080),
        (0x00DF_F180, 0x0F00), // COLOR00
        (0x00DF_F100, if interlace { 4 } else { 0 }),
        (0x00DF_F0A0, 0),
        (0x00DF_F0A2, 0x1000),
        (0x00DF_F0A4, 2),
        (0x00DF_F0A6, 200),
        (0x00DF_F0A8, 64),
        (0x00DF_F096, 0x8201), // DMAEN | AUD0EN, set
    ] {
        immediate(&mut code, 0x33FC, value, address);
    }
    let loop_start = code.len();
    code.extend(0x5279u16.to_be_bytes()); // ADDQ.W #1,$1200
    code.extend(0x1200u32.to_be_bytes());
    for (opcode, source, destination) in [
        (0x33F9u16, 0x00DF_F00Cu32, 0x1202u32), // MOVE.W JOY1DAT,$1202
        (0x13F9, 0x00BF_E001, 0x1204),          // MOVE.B CIA-A PRA,$1204
    ] {
        code.extend(opcode.to_be_bytes());
        code.extend(source.to_be_bytes());
        code.extend(destination.to_be_bytes());
    }
    let displacement = i8::try_from(loop_start as isize - (code.len() + 2) as isize)
        .expect("short owned firmware loop");
    code.extend([0x60, displacement as u8]);
    rom[8..8 + code.len()].copy_from_slice(&code);
    rom
}

fn controls(pressed: bool) -> Vec<InputEvent> {
    vec![
        InputEvent::Key {
            name: "a".into(),
            pressed,
        },
        InputEvent::PointerButton {
            device: "mouse-1".into(),
            button: "left".into(),
            pressed,
        },
        InputEvent::Button {
            port: 2,
            name: "right".into(),
            pressed,
        },
        InputEvent::Button {
            port: 2,
            name: "fire".into(),
            pressed,
        },
    ]
}

#[derive(Default, Serialize)]
struct Audio(Vec<(u64, u32, u8, Vec<u32>)>);
impl AudioSink for Audio {
    fn push_audio(&mut self, packet: AudioPacket<'_>) -> Result<(), MachineError> {
        self.0.push((
            packet.timestamp.get(),
            packet.sample_rate,
            packet.channels,
            packet.samples.iter().map(|v| v.to_bits()).collect(),
        ));
        Ok(())
    }
}
#[derive(Serialize)]
struct Field {
    elapsed: u64,
    complete_frame_sha256: [u8; 32],
    audio: Audio,
}
fn digest(bytes: &[u8]) -> [u8; 32] {
    Sha256::digest(bytes).into()
}
fn field(runtime: &mut AmigaRuntimeKind, events: &[InputEvent], active: bool) -> Field {
    let before = runtime.time().get();
    let mut video = LatestFrameCapture::default();
    let mut audio = Audio::default();
    let result = runtime
        .run_frame(&mut HostIo {
            input_events: events,
            frame_sink: &mut video,
            audio_sink: &mut audio,
            trace_sink: &mut NullTraceSink,
        })
        .expect("native field");
    assert_eq!(result.stop_reason, StopReason::ReachedTarget);
    assert_eq!(video.frames_seen(), 1);
    let frame = video.frame().expect("one completed picture");
    let has_colour = frame
        .pixels
        .as_chunks::<4>()
        .0
        .iter()
        .any(|p| p[..3].iter().any(|v| *v != 0));
    let has_sound = audio
        .0
        .iter()
        .flat_map(|p| &p.3)
        .any(|v| f32::from_bits(*v).abs() > 0.00001);
    assert_eq!(has_colour, active, "guest picture against inactive control");
    assert_eq!(has_sound, active, "guest sound against inactive control");
    Field {
        elapsed: runtime.time().get() - before,
        complete_frame_sha256: digest(&postcard::to_allocvec(frame).expect("complete frame")),
        audio,
    }
}
fn specimen(model: Model, interlace: bool) -> AmigaRuntimeKind {
    let mut runtime = AmigaRuntimeKind::new(model, firmware(true, interlace)).expect("A1200");
    let disk = vec![0x35; 901_120];
    let mut media = MediaSet::new();
    media.push(MediaImage::new("floppy-0", MediaKind::Disk, &disk));
    runtime.load_media(&media).expect("synthetic ADF");
    field(&mut runtime, &[], true);
    field(&mut runtime, &[], true);
    field(&mut runtime, &controls(true), true);
    let target = runtime.time().saturating_add(137);
    runtime
        .run_until(
            target,
            &mut HostIo {
                input_events: &[],
                frame_sink: &mut NullFrameSink,
                audio_sink: &mut NullAudioSink,
                trace_sink: &mut NullTraceSink,
            },
        )
        .expect("half-CCK cut");
    assert!(!runtime.overlay(), "guest removed overlay");
    assert_ne!(runtime.read_word(0x1200), 0, "guest counter advanced");
    assert_ne!(runtime.read_word(0x1202), 0, "guest sampled held joystick");
    assert_eq!(runtime.read_word(0x00BF_E001) & 0x40, 0, "held mouse");
    runtime
}
fn continuation(runtime: &mut AmigaRuntimeKind) -> Vec<u8> {
    let fields = vec![
        field(runtime, &[], true),
        field(runtime, &controls(false), true),
        field(runtime, &[], true),
    ];
    assert_eq!(runtime.read_word(0x1202), 0, "guest saw released joystick");
    assert_ne!(runtime.read_word(0x00BF_E001) & 0x40, 0, "released mouse");
    postcard::to_allocvec(&(fields, digest(&runtime.snapshot().expect("final v66 save"))))
        .expect("native observations")
}
fn capture_directory() -> Option<PathBuf> {
    let directory = std::env::var_os("EMU198X_A1200_CAPTURE")?;
    let head = std::process::Command::new("git")
        .args(["rev-parse", "HEAD"])
        .current_dir(env!("CARGO_MANIFEST_DIR"))
        .output()
        .expect("original HEAD");
    assert!(head.status.success());
    assert_eq!(
        String::from_utf8(head.stdout).expect("HEAD").trim(),
        ORIGINAL_HEAD,
        "frozen fixtures may only be generated before extraction"
    );
    let clean = std::process::Command::new("git")
        .args([
            "diff",
            "--exit-code",
            "--",
            "crates",
            "Cargo.toml",
            "Cargo.lock",
        ])
        .current_dir(Path::new(env!("CARGO_MANIFEST_DIR")).join("../.."))
        .status()
        .expect("tracked source check");
    assert!(
        clean.success(),
        "no edited production source while capturing originals"
    );
    Some(directory.into())
}
#[test]
fn original_v66_native_continuations_remain_exact() {
    let root =
        Path::new(env!("CARGO_MANIFEST_DIR")).join("../../test-data/commodore-amiga/a1200-v66");
    let capture = capture_directory();
    for model in [Model::A1200AgaPal, Model::A1200AgaNtsc] {
        for interlace in [false, true] {
            let name = format!(
                "{}-{}",
                model.model_id(),
                if interlace {
                    "interlaced"
                } else {
                    "progressive"
                }
            );
            let (original, expected) = if let Some(directory) = &capture {
                let mut live = specimen(model, interlace);
                let saved = live.snapshot().expect("original v66 save");
                let output = continuation(&mut live);
                std::fs::create_dir_all(directory).expect("capture directory");
                for (suffix, bytes) in [("state", &saved), ("output", &output)] {
                    use std::io::Write;
                    std::fs::OpenOptions::new()
                        .create_new(true)
                        .write(true)
                        .open(directory.join(format!("{name}.{suffix}")))
                        .expect("new original only")
                        .write_all(bytes)
                        .expect("original bytes");
                }
                (saved, output)
            } else {
                (
                    std::fs::read(root.join(format!("{name}.state")))
                        .expect("immutable original v66"),
                    std::fs::read(root.join(format!("{name}.output")))
                        .expect("original continuation"),
                )
            };
            let mut restored =
                AmigaRuntimeKind::new(model, firmware(true, interlace)).expect("fixed profile");
            restored.restore(&original).expect("old v66 restore");
            assert!(
                restored.snapshot().expect("reencoded") == original,
                "{name}: complete save bytes"
            );
            assert!(
                continuation(&mut restored) == expected,
                "{name}: first frame/audio/clocks/final bytes"
            );
        }
        let mut inactive = AmigaRuntimeKind::new(model, firmware(false, false)).expect("inactive");
        field(&mut inactive, &[], false);
    }
}
