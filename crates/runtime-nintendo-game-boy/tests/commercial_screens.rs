//! Reference-qualified real-game fields. No commercial bytes are committed.
use emu198x_shell::{
    FirmwareImage, FirmwareSet, FramePacket, FrameSink, HostIo, MachineCore, MachineError,
    MediaImage, MediaKind, MediaSet, NullAudioSink, NullTraceSink, PixelFormat, StopReason,
};
use runtime_nintendo_game_boy::{GameBoyRuntime, Model};
use serde::Deserialize;
use sha2::{Digest, Sha256};
use std::{error::Error, path::Path};

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Artifact {
    file: String,
    bytes: usize,
    sha256: String,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Checkpoint {
    frames: u32,
    sha256: String,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Case {
    id: String,
    title: String,
    rom: Artifact,
    checkpoints: Vec<Checkpoint>,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Manifest {
    schema: u8,
    model: String,
    boot: Artifact,
    cases: Vec<Case>,
}

fn sha256(bytes: &[u8]) -> String {
    Sha256::digest(bytes)
        .iter()
        .map(|b| format!("{b:02x}"))
        .collect()
}
fn load(root: &Path, artifact: &Artifact) -> Result<Vec<u8>, Box<dyn Error>> {
    let path = root.join(&artifact.file);
    let bytes = std::fs::read(&path)?;
    assert_eq!(
        bytes.len(),
        artifact.bytes,
        "fixture length: {}",
        path.display()
    );
    assert_eq!(
        sha256(&bytes),
        artifact.sha256,
        "fixture SHA-256: {}",
        path.display()
    );
    Ok(bytes)
}
#[derive(Default)]
struct Picture {
    pixels: Vec<u8>,
    fields: u32,
}
impl FrameSink for Picture {
    fn push_frame(&mut self, frame: FramePacket<'_>) -> Result<(), MachineError> {
        assert_eq!(frame.format, PixelFormat::Indexed8);
        assert_eq!((frame.width, frame.height), (160, 144));
        assert_eq!(frame.pixels.len(), 160 * 144);
        assert!(frame.pixels.iter().all(|p| *p < 4));
        self.pixels = frame.pixels.to_vec();
        self.fields += 1;
        Ok(())
    }
}
fn step(runtime: &mut GameBoyRuntime, picture: &mut Picture) -> Result<bool, MachineError> {
    let before = picture.fields;
    let result = runtime.run_frame(&mut HostIo {
        input_events: &[],
        frame_sink: picture,
        audio_sink: &mut NullAudioSink,
        trace_sink: &mut NullTraceSink,
    })?;
    assert_eq!(result.reached, runtime.time());
    match result.stop_reason {
        StopReason::ReachedTarget => {
            assert_eq!(picture.fields, before + 1);
            Ok(true)
        }
        StopReason::NoFrame => {
            assert_eq!(picture.fields, before);
            Ok(false)
        }
        other => panic!("unexpected stop in a loaded game: {other:?}"),
    }
}
fn advance(
    runtime: &mut GameBoyRuntime,
    picture: &mut Picture,
    count: u32,
) -> Result<(), MachineError> {
    let target = picture.fields + count;
    for _ in 0..(count * 4 + 120) {
        if picture.fields == target {
            return Ok(());
        }
        step(runtime, picture)?;
    }
    panic!("game did not produce {count} bounded native fields");
}
fn run_case(id: &str) -> Result<(), Box<dyn Error>> {
    let manifest: Manifest = serde_json::from_str(include_str!(
        "../../../test-data/game-boy/commercial-screens/expected.json"
    ))?;
    assert_eq!(manifest.schema, 1);
    assert_eq!(manifest.model, "dmg");
    let case = manifest
        .cases
        .iter()
        .find(|case| case.id == id)
        .expect("known case");
    let root = std::env::var_os("EMU198X_GB_COMMERCIAL_ROOT")
        .ok_or("set EMU198X_GB_COMMERCIAL_ROOT to the identified game fixtures")?;
    let boot_root = std::env::var_os("EMU198X_GB_BOOT_ROM_DIR")
        .ok_or("set EMU198X_GB_BOOT_ROM_DIR to the original DMG boot fixture")?;
    let root = Path::new(&root);
    let boot_root = Path::new(&boot_root);
    let rom = load(root, &case.rom)?;
    let boot = load(boot_root, &manifest.boot)?;
    let mut firmware = FirmwareSet::new();
    firmware.push(FirmwareImage::new("dmg-boot-rom", &boot));
    let mut runtime = GameBoyRuntime::from_firmware(Model::Dmg, &firmware)?;
    let mut media = MediaSet::new();
    media.push(MediaImage::new("cartridge", MediaKind::Cartridge, &rom));
    runtime.load_media(&media)?;
    let mut picture = Picture::default();
    let mut anchored = false;
    for _ in 0..1000 {
        let completed = step(&mut runtime, &mut picture)?;
        if completed && runtime.machine().expect("loaded").peek(0xFF50) & 1 != 0 {
            anchored = true;
            break;
        }
    }
    assert!(
        anchored,
        "{} must finish boot and produce a real LCD field",
        case.title
    );
    // SameBoy's first normal callback after BOOT unmapping defines the same origin.
    let mut previous = 0;
    let mut replay = None;
    for checkpoint in &case.checkpoints {
        assert!(
            checkpoint.frames > previous,
            "ordered, advancing checkpoints"
        );
        advance(&mut runtime, &mut picture, checkpoint.frames - previous)?;
        assert_eq!(
            sha256(&picture.pixels),
            checkpoint.sha256,
            "{} at native field {}",
            case.title,
            checkpoint.frames
        );
        if checkpoint.frames == 480 {
            replay = Some(runtime.snapshot()?);
        }
        previous = checkpoint.frames;
    }
    assert_eq!(previous, 720);
    let mut restored = GameBoyRuntime::blank(Model::Dmg);
    restored.restore(&replay.expect("frame-480 snapshot"))?;
    let mut replay_picture = Picture::default();
    advance(&mut restored, &mut replay_picture, 240)?;
    assert_eq!(
        sha256(&replay_picture.pixels),
        case.checkpoints.last().expect("final field").sha256
    );
    assert_eq!(
        runtime.snapshot()?,
        restored.snapshot()?,
        "commercial state must replay exactly"
    );
    assert_eq!(load(root, &case.rom)?, rom, "input ROM remains immutable");
    assert_eq!(
        load(boot_root, &manifest.boot)?,
        boot,
        "boot ROM remains immutable"
    );
    Ok(())
}
#[test]
#[ignore = "FIXTURE: identified Tetris Rev 1 and original DMG boot ROM; see commercial-screens README"]
fn tetris_real_game_screens_match_sameboy_and_replay() -> Result<(), Box<dyn Error>> {
    run_case("tetris")
}
#[test]
#[ignore = "FIXTURE: identified Super Mario Land Rev 1 and original DMG boot ROM; see commercial-screens README"]
fn mario_real_game_screens_match_sameboy_and_replay() -> Result<(), Box<dyn Error>> {
    run_case("mario")
}
