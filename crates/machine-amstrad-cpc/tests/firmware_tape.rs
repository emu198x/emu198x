//! The real firmware loads a real game off a real tape.
//!
//! ```text
//! cargo test --release -p machine-amstrad-cpc --test firmware_tape -- --ignored --nocapture
//! ```
//!
//! Needs the CPC464 firmware at `~/.emu198x/roms/amstrad-cpc/cpc464.rom` and a
//! `.cdt` at `~/.emu198x/media/amstrad-cpc/ascend.cdt` (override with
//! `EMU198X_CPC_CDT`). The default is Inufuto's *Ascend*, a small freely
//! distributed homebrew — chosen because it is short enough to load inside a
//! test and uses `0x11` turbo blocks, so it exercises the block type CPC
//! software actually ships with rather than the Spectrum-standard `0x10`.
//!
//! # Answering the firmware
//!
//! The CPC prompts `Press PLAY then any key:` **between blocks**, not just at
//! the start, and stops the motor while it waits. A test that presses a key
//! once loads exactly one block and then sits forever watching a stopped tape
//! — which looks identical to a broken tape implementation, and was read as
//! one until the screen was actually examined. So this presses a key whenever
//! the motor is off, which is what a person at the keyboard does.

use std::env;
use std::fs;
use std::path::PathBuf;

use machine_amstrad_cpc::AmstradCpc;

fn firmware_path() -> Option<PathBuf> {
    let home = env::var("HOME").ok()?;
    let p = PathBuf::from(home).join(".emu198x/roms/amstrad-cpc/cpc464.rom");
    p.exists().then_some(p)
}

fn tape_path() -> Option<PathBuf> {
    if let Ok(p) = env::var("EMU198X_CPC_CDT") {
        let p = PathBuf::from(p);
        if p.exists() {
            return Some(p);
        }
    }
    let home = env::var("HOME").ok()?;
    let p = PathBuf::from(home).join(".emu198x/media/amstrad-cpc/ascend.cdt");
    p.exists().then_some(p)
}

/// The most common pixel value — the paper a screen is drawn on.
fn dominant_colour(fb: &[u32]) -> u32 {
    let mut counts: std::collections::HashMap<u32, usize> = std::collections::HashMap::new();
    for &px in fb {
        *counts.entry(px).or_default() += 1;
    }
    counts
        .into_iter()
        .max_by_key(|&(_, n)| n)
        .map(|(px, _)| px)
        .unwrap_or(0)
}

fn type_char(cpc: &mut AmstradCpc, c: char) {
    assert!(cpc.press_char(c), "no key produces {c:?}");
    for _ in 0..4 {
        cpc.run_frame();
    }
    cpc.release_char(c);
    for _ in 0..4 {
        cpc.run_frame();
    }
}

#[test]
#[ignore = "needs the CPC464 firmware and a .cdt — run with --ignored"]
fn the_firmware_loads_a_game_from_tape() {
    let (Some(rom), Some(tape)) = (firmware_path(), tape_path()) else {
        panic!("needs ~/.emu198x/roms/amstrad-cpc/cpc464.rom and a .cdt");
    };
    let firmware = fs::read(&rom).expect("read firmware");
    let cdt = fs::read(&tape).expect("read tape");
    let spans = format_amstrad_cpc_cdt::cdt_to_stream(&cdt).expect("parse CDT");
    let total = spans.len();
    eprintln!("tape parsed to {total} spans");

    let mut cpc = AmstradCpc::new(&firmware).expect("build machine");
    for _ in 0..150 {
        cpc.run_frame();
    }
    let boot_paper = dominant_colour(cpc.framebuffer());
    cpc.insert_tape(spans);

    // RUN" with no filename loads and runs the first file on the tape.
    for c in "run\"\r".chars() {
        type_char(&mut cpc, c);
    }

    let mut motor_seen = false;
    let mut idle = 0u32;
    for _ in 0..8_000 {
        cpc.run_frame();
        if cpc.tape_motor_on() {
            motor_seen = true;
            idle = 0;
            continue;
        }
        // Motor stopped: either the firmware is between blocks and wants a
        // key, or the load is finished. Answer, and let the tape position
        // decide which it was.
        idle += 1;
        // Stop answering once the tape is spent: the keys are for the
        // firmware's between-block prompts, and a loaded game reads the
        // keyboard itself — space is one of Ascend's controls.
        if idle.is_multiple_of(20) && cpc.tape().span_position().0 + 200 < total {
            cpc.press_char(' ');
            for _ in 0..3 {
                cpc.run_frame();
            }
            cpc.release_char(' ');
        }
    }

    let (pos, _) = cpc.tape().span_position();
    eprintln!("tape reached {pos} of {total}");
    assert!(motor_seen, "the firmware never started the cassette motor");
    assert!(
        pos * 100 / total >= 99,
        "the tape did not run to the end: {pos} of {total} spans"
    );

    // Let the loaded program start and draw. Ascend spends a while setting up
    // before its first frame, so this is seconds rather than a handful of
    // frames — a shorter wait catches the screen still blank and reads as a
    // failed load.
    for _ in 0..1_500 {
        cpc.run_frame();
    }

    // The loaded program owns the screen now. BASIC boots to blue paper;
    // Ascend clears to black and puts up its title. Comparing the *dominant*
    // colour before and after separates "a program is running" from "BASIC is
    // still showing its banner", without pinning the test to particular pixels
    // or to a colour count — a title screen is legitimately only three
    // colours, which an earlier version of this test mistook for a failed load.
    let after = dominant_colour(cpc.framebuffer());
    eprintln!("paper was {boot_paper:08X}, now {after:08X}");
    assert_ne!(
        after, boot_paper,
        "the screen is still BASIC's — the tape loaded but nothing took over"
    );
}
