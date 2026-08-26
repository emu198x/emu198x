//! VIC-20 end-to-end keyboard test: boot the KERNAL, type a BASIC line
//! through the VIA #2 matrix scan, and read the result off the screen.
//!
//! This exercises the whole input path that was previously dead: VIA #2
//! Timer 1 raising the 60 Hz IRQ, the KERNAL's SCNKEY handler reading the
//! keyboard matrix on port A while driving columns on port B, the
//! keyboard buffer, and BASIC evaluating the line.

use std::env;
use std::fs;
use std::path::PathBuf;

use machine_commodore_vic_20::{Vic20, Vic20Key, Vic20Model};

fn rom(name: &str) -> Option<Vec<u8>> {
    if let Ok(dir) = env::var("EMU198X_VIC20_ROMS") {
        let p = PathBuf::from(dir).join(name);
        if p.exists() {
            return fs::read(p).ok();
        }
    }
    let home = env::var("HOME").ok()?;
    let p = PathBuf::from(home)
        .join(".emu198x/roms/commodore-vic-20")
        .join(name);
    p.exists().then(|| fs::read(p).ok()).flatten()
}

/// Press a key, hold it long enough for at least one IRQ-driven keyboard
/// scan to latch it, then release and settle so the next key reads as a
/// fresh press rather than a held repeat.
fn tap(sys: &mut Vic20, key: Vic20Key) {
    sys.press_key(key);
    for _ in 0..4 {
        sys.run_frame();
    }
    sys.release_key(key);
    for _ in 0..2 {
        sys.run_frame();
    }
}

/// Decode the 22×23 screen-RAM region at $1E00 (unexpanded VIC-20) from
/// screen codes into a printable string.
fn screen_text(sys: &Vic20) -> String {
    (0x1E00u16..0x1FF8)
        .map(|a| {
            let code = sys.peek(a) & 0x7F;
            match code {
                0x01..=0x1A => (b'A' + (code - 1)) as char,
                0x30..=0x39 => (b'0' + (code - 0x30)) as char,
                0x20 => ' ',
                _ => '.',
            }
        })
        .collect()
}

#[test]
#[ignore = "FIXTURE: needs VIC-20 KERNAL/BASIC/char ROMs — run with --ignored"]
fn types_a_basic_line_and_prints_the_result() {
    let (Some(kernal), Some(basic), Some(charrom)) =
        (rom("kernal.rom"), rom("basic.rom"), rom("char.rom"))
    else {
        panic!(
            "VIC-20 ROMs not found — set EMU198X_VIC20_ROMS or place \
             kernal.rom/basic.rom/char.rom at ~/.emu198x/roms/commodore-vic-20/"
        );
    };
    let mut sys = Vic20::new(kernal, basic, charrom, Vic20Model::Pal, 0);

    // Boot to the READY prompt.
    for _ in 0..120 {
        sys.run_frame();
    }

    // Type `PRINT3+4` then RETURN — every key is unshifted, so no SHIFT
    // modelling is needed to drive BASIC.
    for key in [
        Vic20Key::P,
        Vic20Key::R,
        Vic20Key::I,
        Vic20Key::N,
        Vic20Key::T,
        Vic20Key::Num3,
        Vic20Key::Plus,
        Vic20Key::Num4,
        Vic20Key::Return,
    ] {
        tap(&mut sys, key);
    }

    // Let BASIC evaluate and print the answer.
    for _ in 0..30 {
        sys.run_frame();
    }

    let screen = screen_text(&sys);
    // BASIC echoes the typed line and prints the evaluated result `7`.
    // The typed line `PRINT3+4` contains no `7`, so the only `7` on the
    // screen is the answer.
    assert!(
        screen.contains("PRINT3"),
        "expected the typed line to echo on screen; got: {:?}",
        screen.trim()
    );
    assert!(
        screen.contains('7'),
        "expected BASIC to print the result 7; screen was: {:?}",
        screen.trim()
    );
}
