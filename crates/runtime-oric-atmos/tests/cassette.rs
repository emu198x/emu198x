use emu198x_shell::{MachineCore, MachineError, MediaImage, MediaKind, MediaSet, ResetKind};
use runtime_oric_atmos::{Model, OricRuntime};

fn tap() -> Vec<u8> {
    vec![
        0x16, 0x16, 0x16, 0x24, 0, 0, 0x80, 0, 0x40, 0x00, 0x40, 0x00, 0, b'T', 0, 0x60,
    ]
}

fn runtime() -> OricRuntime {
    OricRuntime::new(Model::Atmos, vec![0; 16 * 1024]).expect("synthetic ROM builds")
}

fn load(runtime: &mut OricRuntime, bytes: &[u8]) -> Result<(), MachineError> {
    let mut media = MediaSet::new();
    media.push(MediaImage::new("tape-1", MediaKind::Tape, bytes));
    runtime.load_media(&media)
}

#[test]
fn valid_tap_mounts_and_survives_reset() {
    let mut runtime = runtime();
    load(&mut runtime, &tap()).expect("valid TAP mounts");
    assert!(runtime.machine().expect("machine exists").tape_loaded());
    runtime.reset(ResetKind::Hard);
    assert!(runtime.machine().expect("machine exists").tape_loaded());
}

#[test]
fn malformed_tap_is_invalid_media() {
    let mut runtime = runtime();
    match load(&mut runtime, b"not a TAP") {
        Err(MachineError::InvalidMedia { slot, .. }) => assert_eq!(slot, "tape-1"),
        other => panic!("expected InvalidMedia, got {other:?}"),
    }
}

#[test]
fn unknown_slot_is_rejected() {
    let mut runtime = runtime();
    let bytes = tap();
    let mut media = MediaSet::new();
    media.push(MediaImage::new("tape-2", MediaKind::Tape, &bytes));
    match runtime.load_media(&media) {
        Err(MachineError::UnknownMediaSlot { slot }) => assert_eq!(slot, "tape-2"),
        other => panic!("expected UnknownMediaSlot, got {other:?}"),
    }
}
