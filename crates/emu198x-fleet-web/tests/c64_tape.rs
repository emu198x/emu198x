//! A C64 tape loaded through `Fleet` keeps its contents and loads.
//!
//! Needs the C64 ROMs, from `EMU198X_C64_ROM_DIR` or
//! `~/.emu198x/roms/commodore-c64/`, as the C64 runtime's own tests do.

use std::path::PathBuf;

use emu198x_fleet_web::Fleet;

fn rom_dir() -> Option<PathBuf> {
    let dir = std::env::var("EMU198X_C64_ROM_DIR")
        .map(PathBuf::from)
        .or_else(|_| {
            std::env::var("HOME")
                .map(|home| PathBuf::from(home).join(".emu198x/roms/commodore-c64"))
        })
        .ok()?;
    dir.join("kernal.rom").exists().then_some(dir)
}

/// A TAP holding `payload` as its pulse bytes.
fn tap(payload: &[u8]) -> Vec<u8> {
    let mut bytes = b"C64-TAPE-RAW".to_vec();
    bytes.extend_from_slice(&[1, 0, 0, 0]);
    bytes.extend_from_slice(&u32::try_from(payload.len()).expect("small").to_le_bytes());
    bytes.extend_from_slice(payload);
    bytes
}

/// Fleet once mounted every image writable, which the C64 takes as a blank
/// work tape: the tape read as empty and autoload sat on a blank screen.
#[test]
fn a_c64_tape_keeps_its_pulses() {
    let Some(dir) = rom_dir() else {
        emu198x_test_skip::skip!("C64 ROMs not staged (EMU198X_C64_ROM_DIR)");
    };
    let mut fleet = Fleet::new("commodore-c64", "pal").expect("compiled variant");
    for (id, file) in [
        ("commodore-c64-basic-rom", "basic.rom"),
        ("commodore-c64-kernal-rom", "kernal.rom"),
        ("commodore-c64-character-rom", "chargen.rom"),
    ] {
        let bytes = std::fs::read(dir.join(file)).expect("ROM");
        fleet.firmware(id, &bytes).expect("firmware");
    }
    fleet.boot().expect("boot");
    fleet.load("tape-1", &tap(&[0x30; 64])).expect("tape");
    assert_eq!(fleet.query("tape.pulse_count").expect("query"), "64");
}
