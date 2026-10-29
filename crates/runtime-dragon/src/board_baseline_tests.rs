//! Original v2 envelopes and every native frame/audio sample, captured before extraction.
use super::*;
use emu198x_shell::{AudioPacket, AudioSink, CapturedFrame, FramePacket, FrameSink, NullTraceSink};
use std::path::{Path, PathBuf};
#[derive(Default)]
struct Frames(Vec<CapturedFrame>);
impl FrameSink for Frames {
    fn push_frame(&mut self, p: FramePacket<'_>) -> Result<(), MachineError> {
        self.0.push(CapturedFrame::from_packet(p));
        Ok(())
    }
}
#[derive(Default)]
struct Audio(Vec<u8>);
impl AudioSink for Audio {
    fn push_audio(&mut self, p: AudioPacket<'_>) -> Result<(), MachineError> {
        for s in p.samples {
            self.0.extend(s.to_bits().to_le_bytes());
        }
        Ok(())
    }
}
type Output = (Vec<u8>, Vec<u8>, Vec<u8>);
fn continued(r: &mut DragonRuntime) -> Output {
    let mut frames = Frames::default();
    let mut audio = Audio::default();
    let mut trace = NullTraceSink;
    for _ in 0..3 {
        let result = r
            .run_frame(&mut HostIo {
                input_events: &[],
                frame_sink: &mut frames,
                audio_sink: &mut audio,
                trace_sink: &mut trace,
            })
            .expect("native frame");
        assert_eq!(result.stop_reason, StopReason::ReachedTarget);
    }
    assert_eq!(frames.0.len(), 3);
    assert!(!audio.0.is_empty());
    (
        r.snapshot().expect("continued state"),
        postcard::to_allocvec(&frames.0).expect("complete frames"),
        audio.0,
    )
}
fn prepared(model: Model, native: bool) -> DragonRuntime {
    let rom = std::fs::read(
        Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../../test-data/dragon/board-split-v2/control.rom"),
    )
    .expect("own firmware");
    let mut r = DragonRuntime::new(model, &rom).expect("own ROM");
    if native {
        let mut mode: [u8; ROM_SIZE] = rom.clone().try_into().expect("ROM size");
        mode[0x1000] ^= 1;
        r.dragon64_mode_rom = Some(mode);
        r.machine.install_dragon64_mode_rom(&mode);
    }
    let tape = format198x_dragon_data_dragon_cas::encode_cas(&[
        format198x_dragon_data_dragon_cas::CasBlock::new(1, vec![0x55, 0xAA, 0x33], 128),
        format198x_dragon_data_dragon_cas::CasBlock::new(0xFF, Vec::new(), 128),
    ])
    .expect("own tape");
    let disk = format198x_dragon_data_dragon_disk::DragonDiskImage::from_parts(
        2,
        1,
        18,
        256,
        (0..2 * 18 * 256)
            .map(|a| (a % 251) as u8)
            .collect::<Vec<_>>(),
    )
    .expect("own disk")
    .to_vdk_bytes();
    let mut media = MediaSet::new();
    media.push(emu198x_shell::MediaImage::new(
        "tape-1",
        MediaKind::Tape,
        &tape,
    ));
    media.push(emu198x_shell::MediaImage::new(
        "drive-1",
        MediaKind::Disk,
        &disk,
    ));
    r.load_media(&media).expect("mounted own media");
    r.apply_input_event(&InputEvent::Axis {
        port: 1,
        name: "x".into(),
        value: 12345,
    })
    .expect("axis");
    r.apply_input_event(&InputEvent::Button {
        port: 1,
        name: "fire".into(),
        pressed: true,
    })
    .expect("fire");
    // The guest configures the PIA and drives the DAC; no direct I/O poke.
    let mut code = vec![0x1A, 0x50, 0x10, 0xCE, 0x70, 0]; // ORCC #$50; LDS #$7000
    for (addr, value) in [
        (0xFF21_u16, 0_u8),
        (0xFF20, 0xFC),
        (0xFF21, 0x3C),
        (0xFF23, 0),
        (0xFF22, 0xFF),
        (0xFF23, 0x3C),
        (0xFF22, if native { 0xF8 } else { 0xFC }),
        (0xFF03, 0),
        (0xFF02, 0xFF),
        (0xFF03, 4),
        (0xFF02, 0xFD),
    ] {
        code.extend([0x86, value, 0xB7, (addr >> 8) as u8, addr as u8]);
    }
    code.extend([
        0x86, 0xFC, 0xB7, 0xFF, 0x20, 0x12, 0x86, 0, 0xB7, 0xFF, 0x20, 0x20, 0xF3,
    ]);
    r.machine
        .load_binary_program(0x2000, &code, 0x2000, true)
        .expect("own guest");
    for a in 0..6144_u16 {
        r.machine.poke(a, if a & 1 == 0 { 0x55 } else { 0xAA });
    }
    let mut keyboard = machine_dragon_32::DragonKeyboard::new();
    keyboard
        .press(machine_dragon_32::MatrixKey::new(1, 2))
        .expect("press");
    r.machine.set_keyboard(keyboard);
    if model == Model::Dragon64Pal {
        assert!(r.queue_serial_input(b"board-state"));
    }
    let _ = continued(&mut r);
    for _ in 0..37 {
        r.machine.step_cycle();
    }
    let mut previous = Vec::new();
    r.machine.drain_audio_samples(&mut previous);
    r.audio_buffer.clear();
    if native {
        assert_eq!(
            r.machine.hardware_model(),
            DragonHardwareModel::Dragon64Mode
        );
    }
    r
}
fn exercise(root: &Path, capture: bool) {
    for (model, native) in Model::ALL
        .into_iter()
        .map(|m| (m, false))
        .chain(std::iter::once((Model::Dragon64Pal, true)))
    {
        let id = format!(
            "{}{}",
            model.variant_id(),
            if native { "-native" } else { "" }
        );
        let mut r = prepared(model, native);
        let initial = r.snapshot().expect("initial state");
        let (after, frames, audio) = continued(&mut r);
        for (suffix, value) in ["state", "after.state", "frames", "audio"]
            .into_iter()
            .zip([initial.clone(), after, frames, audio])
        {
            let file = root.join(format!("{id}.{suffix}"));
            if capture {
                std::fs::write(file, value).expect("capture");
            } else {
                assert!(
                    value == std::fs::read(file).expect("original file"),
                    "original {id}.{suffix}"
                );
            }
        }
        let mut restored = DragonRuntime::blank(model);
        restored.restore(&initial).expect("restore original state");
        assert_eq!(restored.snapshot().expect("same state"), initial);
        let (after, frames, audio) = continued(&mut restored);
        for (suffix, value) in ["after.state", "frames", "audio"]
            .into_iter()
            .zip([after, frames, audio])
        {
            let expected =
                std::fs::read(root.join(format!("{id}.{suffix}"))).expect("original output");
            assert!(
                value == expected,
                "{id}.{suffix} differs at {:?}",
                value.iter().zip(&expected).position(|(a, b)| a != b)
            );
        }
    }
}
#[test]
#[ignore = "DIAGNOSTIC: writes original pre-split states to EMU198X_DRAGON_BOARD_CAPTURE; replay runs normally"]
fn capture_original_board_states() {
    let root = PathBuf::from(
        std::env::var_os("EMU198X_DRAGON_BOARD_CAPTURE").expect("explicit output directory"),
    );
    std::fs::create_dir(&root).expect("new capture directory");
    exercise(&root, true);
}
#[test]
fn original_version_2_boards_continue_identically() {
    exercise(
        &Path::new(env!("CARGO_MANIFEST_DIR")).join("../../test-data/dragon/board-split-v2"),
        false,
    );
}

fn original(id: &str) -> Vec<u8> {
    std::fs::read(
        Path::new(env!("CARGO_MANIFEST_DIR"))
            .join(format!("../../test-data/dragon/board-split-v2/{id}.state")),
    )
    .expect("original save")
}
#[test]
fn wrong_profile_embedded_board_and_video_reject_atomically() {
    for target in Model::ALL {
        let initial = original(target.variant_id());
        let mut r = DragonRuntime::blank(target);
        r.restore(&initial).expect("occupied receiver");
        for source in Model::ALL.into_iter().filter(|&m| m != target) {
            let bytes = original(source.variant_id());
            assert!(r.restore(&bytes).is_err());
            assert_eq!(r.snapshot().expect("unchanged"), initial);
            let mut forged: DragonRuntimeSnapshotV1 =
                postcard::from_bytes(&bytes).expect("owning envelope");
            forged.profile_id = profile_for(target).profile_id.as_str().to_owned();
            assert!(
                r.restore(&postcard::to_allocvec(&forged).expect("forged model"))
                    .is_err()
            );
            assert_eq!(r.snapshot().expect("unchanged"), initial);
            forged.model = target;
            assert!(
                r.restore(&postcard::to_allocvec(&forged).expect("forged board"))
                    .is_err()
            );
            assert_eq!(r.snapshot().expect("unchanged"), initial);
        }
        let mut wrong_video: DragonRuntimeSnapshotV1 =
            postcard::from_bytes(&initial).expect("owning envelope");
        wrong_video.machine = wrong_video
            .machine
            .with_video_standard(if target.is_coco() {
                common_dragon::VideoStandard::Pal
            } else {
                common_dragon::VideoStandard::Ntsc
            });
        assert!(
            r.restore(&postcard::to_allocvec(&wrong_video).expect("wrong video"))
                .is_err()
        );
        assert_eq!(r.snapshot().expect("unchanged"), initial);
    }
}
