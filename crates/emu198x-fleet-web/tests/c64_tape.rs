//! C64 tapes through `Fleet`: one loaded keeps its contents, and a blank
//! one takes a SAVE that loads back.
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

/// A booted PAL C64 at READY.
fn booted(dir: &std::path::Path, tape: Option<&[u8]>) -> Fleet {
    let mut fleet = Fleet::new("commodore-c64", "pal").expect("compiled variant");
    for (id, file) in [
        ("commodore-c64-basic-rom", "basic.rom"),
        ("commodore-c64-kernal-rom", "kernal.rom"),
        ("commodore-c64-character-rom", "chargen.rom"),
    ] {
        let bytes = std::fs::read(dir.join(file)).expect("ROM");
        fleet.firmware(id, &bytes).expect("firmware");
    }
    match tape {
        Some(bytes) => fleet.load("tape-1", bytes).expect("tape"),
        None => fleet.blank_tape("tape-1").expect("blank tape"),
    }
    fleet.boot().expect("boot");
    wait_for_text(&mut fleet, "READY.", 600);
    fleet
}

fn screen(fleet: &Fleet) -> String {
    fleet.query("screen.text.lines").expect("screen text")
}

fn wait_for_text(fleet: &mut Fleet, text: &str, max_frames: u32) {
    wait_for_count(fleet, text, 1, max_frames);
}

/// Waits until `text` is on screen `count` times: the READY. after a SAVE
/// or LOAD, below the one from boot.
fn wait_for_count(fleet: &mut Fleet, text: &str, count: usize, max_frames: u32) {
    for _ in 0..max_frames {
        if screen(fleet).matches(text).count() >= count {
            return;
        }
        fleet.step().expect("step");
    }
    panic!(
        "{count} {text:?} not within {max_frames} frames: {}",
        screen(fleet)
    );
}

/// Types `text` with the C64's own keys, holding each for three frames.
fn type_text(fleet: &mut Fleet, text: &str) {
    for ch in text.chars() {
        let keys = runtime_commodore_c64::keys_for_char(ch).expect("typable");
        for (pressed, keys) in [(true, keys.clone()), (false, keys)] {
            for key in keys {
                fleet.key(key, pressed).expect("key");
            }
            for _ in 0..3 {
                fleet.step().expect("step");
            }
        }
    }
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
    let fleet = booted(&dir, Some(&tap(&[0x30; 64])));
    assert_eq!(fleet.query("tape.pulse_count").expect("query"), "64");
}

/// A blank tape takes a SAVE, `export_media` gives it back, and a fresh C64
/// loads and runs it. The program prints a word its listing does not hold,
/// so seeing the word proves the loaded program ran.
#[test]
fn a_blank_tape_takes_a_save_that_loads_back() {
    let Some(dir) = rom_dir() else {
        emu198x_test_skip::skip!("C64 ROMs not staged (EMU198X_C64_ROM_DIR)");
    };
    let mut fleet = booted(&dir, None);
    type_text(&mut fleet, "10 PRINT \"ROUND\"+\"TRIP\"\n");
    type_text(&mut fleet, "SAVE\"RT\"\n");
    wait_for_text(&mut fleet, "PRESS RECORD", 600);
    fleet.transport("tape-1", true).expect("record");
    wait_for_text(&mut fleet, "SAVING RT", 4000);
    wait_for_count(&mut fleet, "READY.", 2, 6000);
    let saved = fleet.export_media("tape-1").expect("a recording");
    assert!(saved.starts_with(b"C64-TAPE-RAW"), "a TAP image");

    let mut fresh = booted(&dir, Some(&saved));
    fresh.autoload_tape(600).expect("autoload");
    wait_for_text(&mut fresh, "FOUND RT", 3000);
    wait_for_count(&mut fresh, "READY.", 2, 8000);
    type_text(&mut fresh, "RUN\n");
    wait_for_text(&mut fresh, "ROUNDTRIP", 600);
}
