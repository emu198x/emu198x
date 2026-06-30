//! Utility-ROM slot ($A000) for the Acorn Atom (#376).

use emu198x_shell::{MachineCore, MachineError, MediaImage, MediaKind, MediaSet, ResetKind};
use runtime_acorn_atom::{AtomRuntime, Model};

fn runtime() -> AtomRuntime {
    AtomRuntime::new(Model::AtomFull, vec![0u8; 24 * 1024])
        .expect("synthetic ROM builds the machine")
}

fn load_rom(rt: &mut AtomRuntime, bytes: &[u8]) -> Result<(), MachineError> {
    let mut media = MediaSet::new();
    media.push(MediaImage::new("rom-pack-1", MediaKind::Cartridge, bytes));
    rt.load_media(&media)
}

#[test]
fn utility_rom_pages_in_and_survives_reset() {
    let mut rt = runtime();
    let mut pack = vec![0u8; 0x1000];
    pack[0] = 0x42;
    pack[0xFFF] = 0x99;
    load_rom(&mut rt, &pack).expect("utility ROM loads");

    assert_eq!(rt.machine().expect("machine").peek_memory(0xA000), 0x42);
    assert_eq!(rt.machine().expect("machine").peek_memory(0xAFFF), 0x99);

    // The toolkit stays plugged across a reset (re-inserted on machine rebuild).
    rt.reset(ResetKind::Hard);
    assert_eq!(
        rt.machine().expect("machine").peek_memory(0xA000),
        0x42,
        "utility ROM survives a reset"
    );
}

#[test]
fn an_oversized_utility_rom_is_rejected() {
    let mut rt = runtime();
    match load_rom(&mut rt, &vec![0u8; 0x1001]) {
        Err(MachineError::InvalidMedia { slot, .. }) => assert_eq!(slot, "rom-pack-1"),
        other => panic!("expected InvalidMedia, got {other:?}"),
    }
}
