//! Real OS-driven cassette load (#386).
//!
//! Boots the genuine BBC MOS + BASIC ROMs, mounts a real UEF tape, types
//! `LOAD""` at the BASIC prompt, and confirms the first file's tokenised BASIC
//! program is read into RAM by the MOS through the 6850 ACIA / interrupt path.
//! The expected bytes are demodulated from the tape itself, so the test is not
//! tied to a specific tape, and RAM is scanned (rather than assuming PAGE) so it
//! is robust to where the MOS places the program.
//!
//! Gated `#[ignore]`: needs the copyrighted MOS + BASIC ROMs (at
//! `~/.emu198x/roms/acorn-bbc-micro/`). The UEF defaults to the in-tree b-em
//! `Welcome_B.uef`; override with `ACORN_UEF`. Run with `--ignored --nocapture`.

use std::env;
use std::fs;
use std::path::PathBuf;

use common_acorn_cassette::{CassetteEvent, CassetteReceiver};
use machine_acorn_bbc_micro::BbcMicro;

fn home() -> PathBuf {
    PathBuf::from(env::var("HOME").expect("HOME set"))
}

fn rom(name: &str) -> Vec<u8> {
    let path = home().join(format!(".emu198x/roms/acorn-bbc-micro/{name}"));
    fs::read(&path).unwrap_or_else(|_| panic!("read {}", path.display()))
}

fn uef_path() -> PathBuf {
    if let Ok(p) = env::var("ACORN_UEF") {
        return PathBuf::from(p);
    }
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../emulators/bbc-micro/b-em/tapes/Welcome_B.uef")
}

/// Demodulate the tape and return the tokenised-BASIC data of its first CFS
/// file (the bytes after the block-0 header), which `LOAD` reads into memory.
fn first_file_program(pulses: Vec<format_acorn_uef::TapePulse>) -> Vec<u8> {
    let mut receiver = CassetteReceiver::new();
    receiver.load(pulses);
    let mut bytes = Vec::new();
    while !receiver.finished() && bytes.len() < 512 {
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

fn press(sys: &mut BbcMicro, col: usize, row: usize) {
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
fn press_shifted(sys: &mut BbcMicro, col: usize, row: usize) {
    sys.press_key(0, 0); // SHIFT
    sys.run_frame();
    sys.press_key(col, row);
    for _ in 0..3 {
        sys.run_frame();
    }
    sys.release_key(col, row);
    sys.run_frame();
    sys.release_key(0, 0);
    for _ in 0..3 {
        sys.run_frame();
    }
}

/// Search RAM (below the MODE 7 screen) for a contiguous byte run.
fn ram_contains(sys: &BbcMicro, needle: &[u8]) -> bool {
    (0x0E00u16..0x7C00 - needle.len() as u16)
        .any(|base| (0..needle.len()).all(|i| sys.peek(base + i as u16) == needle[i]))
}

#[test]
#[ignore = "needs the BBC MOS+BASIC ROMs — run with --ignored"]
fn os_loads_a_real_tape() {
    let os = rom("os.rom");
    let basic = rom("basic.rom");
    assert_eq!(os.len(), 0x4000);
    assert_eq!(basic.len(), 0x4000);

    let uef = fs::read(uef_path()).expect("read UEF");
    let tape = format_acorn_uef::parse(&uef).expect("parse UEF");
    let expected = first_file_program(tape.pulses.clone());
    eprintln!("expected program prefix: {expected:02X?}");

    let mut sys = BbcMicro::new(os);
    sys.insert_rom(15, basic);
    sys.insert_tape(tape.pulses);

    // Boot to the BASIC `>` prompt.
    for _ in 0..300 {
        sys.run_frame();
    }

    // Type LOAD"" <Return>.  L O A D  "  "  <Return>
    press(&mut sys, 6, 5); // L
    press(&mut sys, 6, 3); // O
    press(&mut sys, 1, 4); // A
    press(&mut sys, 2, 3); // D
    press_shifted(&mut sys, 1, 3); // "
    press_shifted(&mut sys, 1, 3); // "
    press(&mut sys, 9, 4); // Return

    // Let the MOS detect the carrier (DCD), spin the tape, and read the file.
    let mut motor_seen = false;
    let mut loaded = false;
    for _ in 0..200 {
        for _ in 0..50 {
            sys.run_frame();
        }
        motor_seen |= sys.cassette_motor_on();
        if ram_contains(&sys, &expected) {
            loaded = true;
            break;
        }
    }

    let screen: String = (0x7C00u16..0x8000)
        .map(|a| {
            let c = sys.peek(a);
            if (0x20..0x7f).contains(&c) {
                c as char
            } else {
                ' '
            }
        })
        .collect();
    eprintln!(
        "screen: {}",
        screen.split_whitespace().collect::<Vec<_>>().join(" ")
    );
    assert!(motor_seen, "the MOS never engaged the cassette motor");
    assert!(loaded, "the tape program never reached RAM");
}
