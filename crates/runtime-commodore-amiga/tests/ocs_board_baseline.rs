//! Immutable original v67 continuations before the RTC correction and OCS split.
mod legacy_v67;
use legacy_v67::Originals;
use std::path::{Path, PathBuf};

use emu198x_shell::{
    AudioPacket, AudioSink, HostIo, InputEvent, LatestFrameCapture, MachineCore, MachineError,
    MediaImage, MediaKind, MediaSet, NullAudioSink, NullFrameSink, NullTraceSink, StopReason,
};
use runtime_commodore_amiga::{AmigaLiveAccess, AmigaRuntimeKind, Model};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

fn standard_firmware(active: bool, interlace: bool) -> Vec<u8> {
    let mut rom = vec![0; 512 * 1024];
    rom[..4].copy_from_slice(&0x0003_F000u32.to_be_bytes());
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
        (0x00DA_8000, 0x00A5),
        (0x00DA_B000, 0x123F),
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

/// Owned bootstrap executes all WOM/RAM writes and the lock transition.
fn firmware(model: Model, active: bool, interlace: bool, boot_visible: bool) -> Vec<u8> {
    let standard = standard_firmware(active, interlace);
    if !model.is_a1000() {
        return standard;
    }
    if boot_visible {
        return standard[..64 * 1024].to_vec();
    }
    let mut wom = standard[..8 + if active { 140 } else { 2 }].to_vec();
    // Copy enough zero tail to include the complete owned active program.
    if active {
        wom = standard[..512].to_vec();
    }
    wom[4..8].copy_from_slice(&0x00FC_0008u32.to_be_bytes());
    let mut boot = vec![0; 64 * 1024];
    boot[..8].copy_from_slice(&standard[..8]);
    let mut code = Vec::new();
    fn word(code: &mut Vec<u8>, value: u16, address: u32) {
        code.extend(0x33FCu16.to_be_bytes());
        code.extend(value.to_be_bytes());
        code.extend(address.to_be_bytes());
    }
    // CIA-A DDRA/PRA: make low RAM available for the lock trampoline.
    for (value, address) in [(3u16, 0x00BF_E201u32), (2, 0x00BF_E001)] {
        code.extend(0x13FCu16.to_be_bytes());
        code.extend(value.to_be_bytes());
        code.extend(address.to_be_bytes());
    }
    for (index, bytes) in wom.as_chunks::<2>().0.iter().enumerate() {
        word(
            &mut code,
            u16::from_be_bytes(*bytes),
            0x00FC_0000 + index as u32 * 2,
        );
    }
    // Execute the lock write from RAM so subsequent prefetch cannot consume
    // a different instruction when the bootstrap ROM disappears.
    let trampoline = [0x33FCu16, 0, 0x00F8, 0, 0x4EF9, 0x00FC, 8];
    for (index, value) in trampoline.into_iter().enumerate() {
        word(&mut code, value, 0x4000 + index as u32 * 2);
    }
    code.extend(0x4EF9u16.to_be_bytes());
    code.extend(0x4000u32.to_be_bytes());
    boot[8..8 + code.len()].copy_from_slice(&code);
    boot
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

#[derive(Default, Serialize, Deserialize)]
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
#[derive(Serialize, Deserialize)]
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
fn specimen(
    model: Model,
    interlace: bool,
    debug_cut: bool,
    boot_visible: bool,
) -> AmigaRuntimeKind {
    let mut runtime = AmigaRuntimeKind::new(model, firmware(model, true, interlace, boot_visible))
        .expect("original OCS-shaped profile");
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
        .expect("original short time request");
    if debug_cut {
        for _ in 0..2000 {
            runtime
                .debug_target_mut()
                .expect("live debugger")
                .step_instruction();
            if runtime.scheduler_diagnostic_snapshot().cck_phase == 1 {
                break;
            }
        }
        assert_eq!(
            runtime.scheduler_diagnostic_snapshot().cck_phase,
            1,
            "actual half-CCK cut"
        );
        assert_eq!(
            runtime.time().get(),
            runtime.tick_count(),
            "debugger accounts machine ticks"
        );
    } else {
        assert_eq!(
            runtime.scheduler_diagnostic_snapshot().cck_phase,
            0,
            "original request stopped at a CCK boundary"
        );
    }
    if model.is_a1000() {
        let rom = runtime
            .memory_diagnostic_snapshot()
            .rom
            .a1000
            .expect("A1000 fitting");
        assert_eq!(rom.boot_rom_visible, boot_visible, "owned guest boot phase");
        assert_eq!(rom.wom_locked, !boot_visible, "owned guest lock transition");
    }
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
    postcard::to_allocvec(&(fields, digest(&runtime.snapshot().expect("final v67 save"))))
        .expect("native observations")
}
fn diagnose(name: &str, actual: &[u8], expected: &[u8]) {
    let (a, final_a): (Vec<Field>, [u8; 32]) =
        postcard::from_bytes(actual).expect("actual observations");
    let (e, final_e): (Vec<Field>, [u8; 32]) =
        postcard::from_bytes(expected).expect("original observations");
    assert_eq!(a.len(), e.len());
    for (index, (a, e)) in a.iter().zip(&e).enumerate() {
        let mut differences = 0;
        let mut max_ulp = 0;
        let mut max_error = 0.0_f32;
        for (a, e) in a.audio.0.iter().zip(&e.audio.0) {
            for (a, e) in a.3.iter().zip(&e.3) {
                differences += usize::from(a != e);
                max_ulp = max_ulp.max(a.abs_diff(*e));
                max_error = max_error.max((f32::from_bits(*a) - f32::from_bits(*e)).abs());
            }
        }
        eprintln!(
            "{name}: field {index}: clocks {}/{}, frame exact={}, audio packets {}/{}, differing samples={differences}, max bit-distance={max_ulp}, max absolute error={max_error:e}",
            a.elapsed,
            e.elapsed,
            a.complete_frame_sha256 == e.complete_frame_sha256,
            a.audio.0.len(),
            e.audio.0.len()
        );
    }
    eprintln!("{name}: complete final save exact={}", final_a == final_e);
}

fn models() -> [Model; 12] {
    [
        Model::A1000OcsPal,
        Model::A1000OcsNtsc,
        Model::A500OcsPal,
        Model::A500OcsNtsc,
        Model::A500OcsPalA501,
        Model::A500OcsNtscA501,
        Model::A500OcsPalMaxed,
        Model::A500OcsNtscMaxed,
        Model::A2000OcsPal,
        Model::A2000OcsNtsc,
        Model::A500OcsPalGvpA530,
        Model::A500OcsNtscGvpA530,
    ]
}
#[test]
fn original_v67_native_and_boot_phase_continuations_remain_exact() {
    let root = std::env::var_os("EMU198X_OCS_FIXTURE_ROOT").map_or_else(
        || Path::new(env!("CARGO_MANIFEST_DIR")).join("../../test-data/commodore-amiga/ocs-v67"),
        PathBuf::from,
    );
    let mut originals = Originals::open(&root);
    for (kind, model, boot_visible) in [
        ("kickstart", Model::A500OcsPal, false),
        ("bootstrap", Model::A1000OcsPal, true),
        ("handoff", Model::A1000OcsPal, false),
    ] {
        for (name, active, interlace) in [
            ("active-progressive", true, false),
            ("active-interlaced", true, true),
            ("inactive", false, false),
        ] {
            let bytes = firmware(model, active, interlace, boot_visible);
            assert!(
                originals.read(&format!("{kind}-{name}.rom")) == bytes,
                "{kind}-{name}: owned firmware bytes"
            );
        }
    }
    let mut cases = 0;
    for model in models() {
        let phases: &[bool] = if model.is_a1000() {
            &[true, false]
        } else {
            &[false]
        };
        for &boot_visible in phases {
            for interlace in [false, true] {
                for debug_cut in [false, true] {
                    let name = format!(
                        "{}-{}-{}{}",
                        model.model_id(),
                        if model.is_a1000() {
                            if boot_visible {
                                "bootstrap"
                            } else {
                                "wom-locked"
                            }
                        } else {
                            "kickstart"
                        },
                        if interlace {
                            "interlaced"
                        } else {
                            "progressive"
                        },
                        if debug_cut { "-debug-cut" } else { "" }
                    );
                    let mut live = specimen(model, interlace, debug_cut, boot_visible);
                    let original = originals.read(&format!("{name}.state"));
                    assert_eq!(original.first(), Some(&67));
                    let expected = originals.read(&format!("{name}.output"));
                    let final_state = originals.read(&format!("{name}.final-state"));
                    // Cold construction seeds the RTC once from host time. Its
                    // native guest output must agree; exact persisted history
                    // is checked independently on the restored path below.
                    let live_output = continuation(&mut live);
                    let (live_fields, _): (Vec<Field>, [u8; 32]) =
                        postcard::from_bytes(&live_output).expect("fresh guest packets");
                    let (original_fields, _): (Vec<Field>, [u8; 32]) =
                        postcard::from_bytes(&expected).expect("frozen guest packets");
                    let live_fields_bytes =
                        postcard::to_allocvec(&live_fields).expect("fresh fields");
                    let original_fields_bytes =
                        postcard::to_allocvec(&original_fields).expect("original fields");
                    if live_fields_bytes != original_fields_bytes {
                        eprintln!(
                            "{name}: fresh clocks {:?}; expected clocks {:?}",
                            live_fields
                                .iter()
                                .map(|field| field.elapsed)
                                .collect::<Vec<_>>(),
                            original_fields
                                .iter()
                                .map(|field| field.elapsed)
                                .collect::<Vec<_>>()
                        );
                    }
                    assert!(
                        live_fields_bytes == original_fields_bytes,
                        "{name}: every independently constructed video/audio/clock packet"
                    );
                    let mut restored = AmigaRuntimeKind::new(
                        model,
                        firmware(model, true, interlace, boot_visible),
                    )
                    .expect("selected profile");
                    restored.restore(&original).expect("original v67 restore");
                    assert!(
                        restored.snapshot().expect("re-encode original") == original,
                        "{name}: every original saved byte before correction"
                    );
                    assert_eq!(
                        restored.scheduler_diagnostic_snapshot().cck_phase,
                        u8::from(debug_cut)
                    );
                    let actual = continuation(&mut restored);
                    let (_, expected_final_hash): (Vec<Field>, [u8; 32]) =
                        postcard::from_bytes(&expected).expect("original packets");
                    assert_eq!(digest(&final_state), expected_final_hash);
                    assert!(
                        restored.snapshot().expect("complete resumed save") == final_state,
                        "{name}: every final saved byte before correction"
                    );
                    if actual != expected {
                        diagnose(&name, &actual, &expected);
                    }
                    assert!(
                        actual == expected,
                        "{name}: all original video/audio/clock packets"
                    );
                    cases += 1;
                    eprintln!("original case {cases}: {name} exact");
                }
            }
            let mut inactive =
                AmigaRuntimeKind::new(model, firmware(model, false, false, boot_visible))
                    .expect("inactive control");
            field(&mut inactive, &[], false);
        }
    }
    assert_eq!(
        cases, 56,
        "twelve profiles, both A1000 boot phases, video modes and cuts"
    );
}
