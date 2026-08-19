//! Loading a `.o` tape through the ROM's own cassette loader.
//!
//! Nothing here injects bytes into RAM. The image is encoded as a pulse
//! train, presented on bit 7 of the port, and decoded by Sinclair's loader
//! reading the bus — the same path a cassette recorder would drive.
//!
//! # The leader is not padding
//!
//! The ROM will not start decoding until the line has been quiet for a
//! `$5712` countdown at `$0207`, and **any** high resets it. A tape whose
//! pulses begin too soon plays out entirely inside that leader search, and
//! the loader then waits forever for a signal that has already gone. That
//! failure looks exactly like a broken decoder — the tape is consumed, RAM
//! is untouched — which is what it looked like here for some time. The tell
//! is that the port reads all come from `$0226`, the wait loop, and none
//! from `$0234`, the bit-measurement loop.

use std::env;
use std::fs;
use std::path::PathBuf;

use format_sinclair_zx80_o::Zx80Image;
use machine_sinclair_zx80::{Zx80, Zx80Key};

/// Two bytes of the system variables track running time, so a machine that
/// has been on for a different length of time disagrees there. They are the
/// only bytes that differ across a load, and excluding them by name is more
/// honest than loosening the comparison.
const RUNNING_COUNTER: std::ops::RangeInclusive<usize> = 30..=31;

fn rom_path() -> Option<PathBuf> {
    if let Ok(path) = env::var("EMU198X_ZX80_ROM") {
        let path = PathBuf::from(path);
        if path.exists() {
            return Some(path);
        }
    }
    let home = env::var("HOME").ok()?;
    let path = PathBuf::from(home).join(".emu198x/roms/sinclair-zx80/zx80.rom");
    path.exists().then_some(path)
}

fn boot(rom: &[u8]) -> Zx80 {
    let mut machine = Zx80::new(rom.to_vec(), 16 * 1024).expect("init");
    for _ in 0..250 {
        machine.run_frame();
    }
    machine
}

fn press(machine: &mut Zx80, key: Zx80Key) {
    machine.press_key(key);
    for _ in 0..6 {
        machine.run_frame();
    }
    machine.release_key(key);
    for _ in 0..60 {
        machine.run_frame();
    }
}

/// Compares two images, ignoring the bytes that track running time.
fn same_program(a: &[u8], b: &[u8]) -> bool {
    a.len() == b.len()
        && a.iter()
            .zip(b)
            .enumerate()
            .all(|(offset, (x, y))| RUNNING_COUNTER.contains(&offset) || x == y)
}

/// RAM from `$4000` to the address in `$400A` — what `SAVE` puts on tape.
fn image(machine: &Zx80) -> Vec<u8> {
    let end =
        u16::from(machine.peek_memory(0x400A)) | (u16::from(machine.peek_memory(0x400B)) << 8);
    (0x4000..end).map(|a| machine.peek_memory(a)).collect()
}

#[test]
#[ignore = "needs a 4 KB ZX80 ROM — run with --ignored"]
fn a_saved_program_loads_back_through_the_rom() {
    let Some(path) = rom_path() else {
        emu198x_test_skip::skip!(
            "ZX80 ROM not staged — set EMU198X_ZX80_ROM or place zx80.rom at ~/.emu198x/roms/sinclair-zx80/"
        );
    };
    let rom = fs::read(&path).expect("read ROM");

    // A program, so the comparison means something: an empty one is what a
    // fresh boot already holds, and would pass without loading anything.
    let mut authored = boot(&rom);
    for key in [Zx80Key::N1, Zx80Key::Y, Zx80Key::Newline] {
        press(&mut authored, key);
    }
    let saved = image(&authored);
    assert!(
        saved.len() > 41,
        "typing `1 REM` should grow the program past an empty one; got {} bytes",
        saved.len()
    );

    let parsed = Zx80Image::parse(&saved).expect("a real save should parse");
    let pulses = parsed.to_pulses();

    // W is LOAD on a ZX80. The tape is threaded first and plays while the
    // user types, as it would on a recorder left running.
    let mut loading = boot(&rom);
    let before = image(&loading);
    assert_ne!(before, saved, "the fresh machine must not already hold it");

    // Type LOAD *first*, then start the tape. The encoder's lead-in covers
    // the loader's leader countdown, not a user's typing: threading the tape
    // first spends the whole quiet run before the ROM is listening.
    press(&mut loading, Zx80Key::W);
    press(&mut loading, Zx80Key::Newline);
    loading.insert_tape(&pulses);
    for _ in 0..3000 {
        loading.run_frame();
    }

    let loaded = image(&loading);
    assert_eq!(
        loaded.len(),
        saved.len(),
        "the loaded program should be the same size as the saved one"
    );
    for (offset, (want, got)) in saved.iter().zip(&loaded).enumerate() {
        if RUNNING_COUNTER.contains(&offset) {
            continue;
        }
        assert_eq!(
            got, want,
            "byte {offset} differs: the tape decoded wrongly from there"
        );
    }
}

/// A tape that is consumed but never decoded is the failure this cost the
/// most time to diagnose, so it gets its own assertion: the loader has to
/// still be waiting, not finished.
#[test]
#[ignore = "needs a 4 KB ZX80 ROM — run with --ignored"]
fn a_tape_with_no_leader_is_missed_entirely() {
    let Some(path) = rom_path() else {
        emu198x_test_skip::skip!("ZX80 ROM not staged");
    };
    let rom = fs::read(&path).expect("read ROM");

    let saved = image(&boot(&rom));
    let parsed = Zx80Image::parse(&saved).expect("parses");
    // Strip the lead-in: pulses now start immediately, inside the leader
    // search, and every one of them resets its countdown.
    let lead_in = format_sinclair_zx80_o::LEAD_IN_T;
    let no_leader: Vec<u64> = parsed
        .to_pulses()
        .into_iter()
        .map(|t| t - lead_in + 1_000)
        .collect();

    let mut loading = boot(&rom);
    press(&mut loading, Zx80Key::W);
    press(&mut loading, Zx80Key::Newline);
    loading.insert_tape(&no_leader);
    for _ in 0..1_500 {
        loading.run_frame();
    }

    assert_eq!(
        loading.tape_remaining(),
        0,
        "the tape should have played out"
    );
    assert!(
        same_program(&image(&loading), &saved),
        "and nothing should have been decoded from it"
    );
}
