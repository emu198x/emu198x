//! `.atm` program-image loading for the Acorn Atom (#366).
//!
//! Drives the runtime's `MediaKind::Program` / `program-1` path with synthetic
//! `.atm` images, so no external firmware or media is needed.

use emu198x_shell::{MachineCore, MachineError, MediaImage, MediaKind, MediaSet};
use runtime_acorn_atom::{AtomRuntime, Model};

/// Build a `.atm`: 16-byte name, LE load/exec/length, then the body.
fn atm(name: &str, load: u16, exec: u16, body: &[u8]) -> Vec<u8> {
    let mut image = vec![0u8; 16];
    let name = name.as_bytes();
    let n = name.len().min(16);
    image[..n].copy_from_slice(&name[..n]);
    image.extend_from_slice(&load.to_le_bytes());
    image.extend_from_slice(&exec.to_le_bytes());
    image.extend_from_slice(&(body.len() as u16).to_le_bytes());
    image.extend_from_slice(body);
    image
}

fn runtime() -> AtomRuntime {
    AtomRuntime::new(Model::AtomFull, vec![0u8; 24 * 1024])
        .expect("synthetic ROM builds the machine")
}

fn load(rt: &mut AtomRuntime, bytes: &[u8]) -> Result<(), MachineError> {
    let mut media = MediaSet::new();
    media.push(MediaImage::new("program-1", MediaKind::Program, bytes));
    rt.load_media(&media)
}

#[test]
fn atm_loads_into_ram_and_autoruns() {
    let mut rt = runtime();
    // LDA #$42 ; STA $80 ; JMP * (loop) — at $0200.
    let program = [0xA9, 0x42, 0x85, 0x80, 0x4C, 0x04, 0x02];
    load(&mut rt, &atm("TEST", 0x0200, 0x0200, &program)).expect(".atm loads");

    assert_eq!(
        rt.machine().expect("machine").peek(0x0200),
        0xA9,
        "program bytes land in RAM at the load address"
    );

    // exec_address is in low RAM, so the program auto-runs: it writes 0x42 to $80.
    let machine = rt.machine_mut().expect("machine");
    for _ in 0..3 {
        machine.run_frame();
    }
    assert_eq!(machine.peek(0x0080), 0x42, "the loaded program ran");
}

#[test]
fn a_screen_atm_loads_into_video_ram_without_running_it() {
    let mut rt = runtime();
    // exec in video RAM ($8000) => load only, no jump into screen data.
    load(&mut rt, &atm("SCREEN", 0x8000, 0x8000, &[0x11, 0x22, 0x33])).expect("loads");
    let machine = rt.machine().expect("machine");
    assert_eq!(machine.peek(0x8000), 0x11);
    assert_eq!(machine.peek(0x8002), 0x33);
}

#[test]
fn an_oversized_atm_reports_invalid_media() {
    let mut rt = runtime();
    // $7800 + 0x1000 bytes runs past the 32 KB RAM ceiling ($8000).
    match load(&mut rt, &atm("BIG", 0x7800, 0x7800, &vec![0u8; 0x1000])) {
        Err(MachineError::InvalidMedia { slot, .. }) => assert_eq!(slot, "program-1"),
        other => panic!("expected InvalidMedia, got {other:?}"),
    }
}

#[test]
fn a_corrupt_atm_reports_invalid_media() {
    let mut rt = runtime();
    match load(&mut rt, b"short") {
        Err(MachineError::InvalidMedia { slot, .. }) => assert_eq!(slot, "program-1"),
        other => panic!("expected InvalidMedia, got {other:?}"),
    }
}

/// Load a real `.atm` (e.g. from the Atom Software Archive) and confirm its first
/// body byte lands at the header's load address.
#[test]
#[ignore = "needs a real .atm — set EMU198X_ATOM_ATM"]
fn real_atm_loads_to_its_address() {
    let path = std::env::var("EMU198X_ATOM_ATM").expect("set EMU198X_ATOM_ATM to a .atm file");
    let bytes = std::fs::read(&path).expect("read the .atm");
    let load_address = u16::from_le_bytes([bytes[16], bytes[17]]);
    let first_body_byte = bytes[22];

    let mut rt = runtime();
    load(&mut rt, &bytes).expect("the real .atm loads");
    assert_eq!(
        rt.machine().expect("machine").peek(load_address),
        first_body_byte,
        "the first body byte landed at the load address"
    );
}
