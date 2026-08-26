//! Real OS-driven cassette load (#394).
//!
//! Boots the genuine Electron OS + BASIC ROMs, mounts a real UEF tape, types
//! `LOAD""` at the BASIC prompt, and verifies the first file's tokenised BASIC
//! program lands at `PAGE` (`&0E00`) — proving the OS reads the tape through the
//! ULA's `$FE04` / interrupt path. The expected bytes are demodulated from the
//! tape itself, so the test is not tied to a specific game.
//!
//! Gated `#[ignore]`: needs the copyrighted OS + BASIC ROMs (at
//! `~/.emu198x/roms/acorn-electron/`) and a UEF via `ACORN_UEF` (defaults to a
//! staged game). Run with `--ignored --nocapture`.

use std::env;
use std::fs;
use std::path::PathBuf;

use common_acorn_cassette::{CassetteEvent, CassetteReceiver};
use machine_acorn_electron::AcornElectron;

const PAGE: u16 = 0x0E00;

fn home() -> PathBuf {
    PathBuf::from(env::var("HOME").expect("HOME set"))
}

fn rom(name: &str) -> Vec<u8> {
    let path = home().join(format!(".emu198x/roms/acorn-electron/{name}"));
    fs::read(&path).unwrap_or_else(|_| panic!("read {}", path.display()))
}

fn uef_path() -> PathBuf {
    if let Ok(p) = env::var("ACORN_UEF") {
        return PathBuf::from(p);
    }
    home().join(".emu198x/media/acorn-electron/Thrust (1986)(Superior Software).uef")
}

/// Demodulate the tape and return the tokenised-BASIC data of its first CFS
/// file (the bytes after the block-0 header), which `LOAD` stores at `PAGE`.
fn first_file_program(pulses: Vec<format_acorn_uef::TapePulse>) -> Vec<u8> {
    let mut receiver = CassetteReceiver::new();
    receiver.load(pulses);
    let mut bytes = Vec::new();
    while !receiver.finished() {
        receiver.advance(10_000_000, &mut |event| {
            if let CassetteEvent::ByteReady(byte) = event {
                bytes.push(byte);
            }
        });
    }
    let sync = bytes
        .iter()
        .position(|&b| b == 0x2A)
        .expect("CFS sync byte");
    let name_end = bytes[sync + 1..]
        .iter()
        .position(|&b| b == 0)
        .expect("filename terminator")
        + sync
        + 1;
    // After the 0x00 terminator: 17 header bytes + 2 header-CRC bytes, then data.
    let data_start = name_end + 1 + 17 + 2;
    bytes[data_start..data_start + 16].to_vec()
}

fn press(sys: &mut AcornElectron, col: usize, row: usize) {
    sys.press_key(col, row);
    for _ in 0..3 {
        sys.run_frame();
    }
    sys.release_key(col, row);
    for _ in 0..3 {
        sys.run_frame();
    }
}

/// SHIFT + key (for `"` = SHIFT+2).
fn press_shifted(sys: &mut AcornElectron, col: usize, row: usize) {
    sys.press_key(13, 3); // SHIFT
    sys.run_frame();
    sys.press_key(col, row);
    for _ in 0..3 {
        sys.run_frame();
    }
    sys.release_key(col, row);
    sys.run_frame();
    sys.release_key(13, 3);
    for _ in 0..3 {
        sys.run_frame();
    }
}

#[test]
#[ignore = "FIXTURE: needs the Electron OS+BASIC ROMs and a UEF (ACORN_UEF) — run with --ignored"]
fn os_loads_a_real_tape_to_page() {
    let os = rom("os.rom");
    let basic = rom("basic.rom");
    assert_eq!(os.len(), 0x4000);
    assert_eq!(basic.len(), 0x4000);

    let uef = fs::read(uef_path()).expect("read UEF");
    let tape = format_acorn_uef::parse(&uef).expect("parse UEF");
    let expected = first_file_program(tape.pulses.clone());
    eprintln!("expected program prefix at PAGE: {expected:02X?}");

    let mut sys = AcornElectron::new(os, basic);
    sys.insert_tape(tape.pulses);

    // Boot to the BASIC `>` prompt.
    for _ in 0..300 {
        sys.run_frame();
    }

    // Type LOAD"" <Return>.  L O A D  "  "  <Return>
    press(&mut sys, 4, 2); // L
    press(&mut sys, 4, 1); // O
    press(&mut sys, 12, 2); // A
    press(&mut sys, 10, 2); // D
    press_shifted(&mut sys, 11, 0); // "
    press_shifted(&mut sys, 11, 0); // "
    press(&mut sys, 1, 2); // Return

    // Let the OS spin the tape and read the file. Poll for the program to land
    // at PAGE (a deep token byte the empty boot program never has).
    let mut motor_seen = false;
    let mut loaded = false;
    for _ in 0..120 {
        for _ in 0..50 {
            sys.run_frame();
        }
        motor_seen |= sys.cassette_motor_on();
        if sys.peek(PAGE + 1) == expected[1]
            && sys.peek(PAGE + 2) == expected[2]
            && sys.peek(PAGE + 4) == expected[4]
        {
            loaded = true;
            break;
        }
    }

    let got: Vec<u8> = (0..16).map(|i| sys.peek(PAGE + i)).collect();
    eprintln!("motor engaged: {motor_seen}; bytes at PAGE: {got:02X?}");
    assert!(motor_seen, "the OS never engaged the cassette motor");
    assert!(loaded, "program did not reach PAGE; got {got:02X?}");
    assert_eq!(&got, &expected, "loaded bytes differ from the tape");
}
