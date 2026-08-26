//! Tatung Einstein end-to-end keyboard test: boot the MOS, type a word
//! through the AY-driven keyboard, and read it off the VDP screen.
//!
//! Exercises the whole input path that was dead until the I/O map was
//! corrected (AY data/select on $02/$03; keyboard interrupt mask on $20)
//! and the keyboard was wired to the AY-3-8910 ports: port A drives the
//! row select, port B reads the columns, and a ~50 Hz IM 2 interrupt
//! (vector $F7) drives the MOS scan handler.

use std::env;
use std::fs;
use std::path::PathBuf;

use machine_tatung_einstein::{Einstein, EinsteinRegion};

fn rom() -> Option<Vec<u8>> {
    if let Ok(p) = env::var("EMU198X_EINSTEIN_BIOS") {
        let p = PathBuf::from(p);
        if p.exists() {
            return fs::read(p).ok();
        }
    }
    let home = env::var("HOME").ok()?;
    let p = PathBuf::from(home).join(".emu198x/roms/tatung-einstein/einstein.rom");
    p.exists().then(|| fs::read(p).ok()).flatten()
}

fn tap(sys: &mut Einstein, row: usize, col: u8) {
    sys.press_key(row, col);
    for _ in 0..8 {
        sys.run_frame();
    }
    sys.release_key(row, col);
    for _ in 0..6 {
        sys.run_frame();
    }
}

fn screen(sys: &Einstein) -> String {
    sys.vdp()
        .vram()
        .iter()
        .map(|&c| {
            if (0x20..0x7f).contains(&c) {
                c as char
            } else {
                ' '
            }
        })
        .collect::<String>()
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
}

#[test]
#[ignore = "FIXTURE: needs Tatung Einstein X-TAL MOS ROM — run with --ignored"]
fn types_hello_at_the_mos_prompt() {
    let Some(bios) = rom() else {
        panic!(
            "Einstein BIOS not found — set EMU198X_EINSTEIN_BIOS or place \
             einstein.rom at ~/.emu198x/roms/tatung-einstein/"
        );
    };
    let mut sys = Einstein::new(bios, EinsteinRegion::Pal);
    for _ in 0..300 {
        sys.run_frame();
    }

    // H E L L O via the keyboard matrix.
    for (row, col) in [(6usize, 1u8), (5, 4), (2, 1), (2, 1), (1, 1)] {
        tap(&mut sys, row, col);
    }
    for _ in 0..20 {
        sys.run_frame();
    }

    let screen = screen(&sys);
    assert!(
        screen.contains("HELLO"),
        "expected the typed word on the MOS screen; got: {screen:?}"
    );
}
