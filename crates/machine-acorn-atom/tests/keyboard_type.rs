//! Acorn Atom end-to-end keyboard test: boot the MOS, type a BASIC line
//! through the 8255-scanned keyboard, and read the result off the screen.
//!
//! Exercises the path that was dead until the Intel 8255 PPI replaced the
//! 6520 stand-in and the VDG field-sync was synthesised on port C: port A
//! drives the 4-to-10 row decoder, port B reads the columns, and the MOS
//! times its scan off the field-sync pulse.

use std::env;
use std::fs;
use std::path::PathBuf;

use machine_acorn_atom::{AcornAtom, AtomKey};

fn rom() -> Option<Vec<u8>> {
    if let Ok(p) = env::var("EMU198X_ATOM_ROM") {
        let p = PathBuf::from(p);
        if p.exists() {
            return fs::read(p).ok();
        }
    }
    let home = env::var("HOME").ok()?;
    let p = PathBuf::from(home).join(".emu198x/roms/acorn-atom/atom.rom");
    p.exists().then(|| fs::read(p).ok()).flatten()
}

fn tap(sys: &mut AcornAtom, key: AtomKey) {
    sys.press_key(key);
    sys.run_frame();
    sys.release_key(key);
    for _ in 0..5 {
        sys.run_frame();
    }
}

fn screen(sys: &AcornAtom) -> String {
    (0x8000u16..0x8280)
        .map(|a| {
            let c = sys.peek(a);
            match c {
                0x01..=0x1A => (b'@' + c) as char,
                0x20..=0x3F => c as char,
                _ => ' ',
            }
        })
        .collect::<String>()
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
}

#[test]
#[ignore = "needs Acorn Atom ROM — run with --ignored"]
fn types_a_basic_line_and_prints_the_result() {
    let Some(rom) = rom() else {
        panic!(
            "Atom ROM not found — set EMU198X_ATOM_ROM or place atom.rom \
             at ~/.emu198x/roms/acorn-atom/"
        );
    };
    let mut sys = AcornAtom::new(rom, 0x0A00);
    for _ in 0..120 {
        sys.run_frame();
    }

    // `P.9/3` is `PRINT 9/3` in Atom BASIC — all unshifted keys — and
    // integer division prints 3.
    for key in [
        AtomKey::P,
        AtomKey::Period,
        AtomKey::Num9,
        AtomKey::Slash,
        AtomKey::Num3,
        AtomKey::Return,
    ] {
        tap(&mut sys, key);
    }
    for _ in 0..20 {
        sys.run_frame();
    }

    let screen = screen(&sys);
    assert!(
        screen.contains("P.9/3"),
        "expected the typed line to echo; got: {screen:?}"
    );
    assert!(
        screen.contains('3'),
        "expected BASIC to print the result 3; screen was: {screen:?}"
    );
}
