//! PET end-to-end keyboard test: boot the editor/BASIC, type a line
//! through the PIA-driven keyboard scan, and read the result off the
//! screen.
//!
//! Exercises the whole input path that was previously dead: PIA #1 CB1
//! taking the CRTC vertical-retrace edge to raise the 60 Hz IRQ, the
//! editor's keyboard scan driving a binary row number on port A and
//! reading the columns on port B, and BASIC evaluating the line.

use std::env;
use std::fs;
use std::path::PathBuf;

use machine_commodore_pet::{Pet, PetKey};

fn rom(name: &str) -> Option<Vec<u8>> {
    if let Ok(dir) = env::var("EMU198X_PET_ROMS") {
        let p = PathBuf::from(dir).join(name);
        if p.exists() {
            return fs::read(p).ok();
        }
    }
    let home = env::var("HOME").ok()?;
    let p = PathBuf::from(home)
        .join(".emu198x/roms/commodore-pet")
        .join(name);
    p.exists().then(|| fs::read(p).ok()).flatten()
}

fn tap(sys: &mut Pet, key: PetKey) {
    sys.press_key(key);
    for _ in 0..5 {
        sys.run_frame();
    }
    sys.release_key(key);
    for _ in 0..5 {
        sys.run_frame();
    }
}

/// Raw screen codes for one 40-column line, so a test can look at what was
/// actually deposited rather than at a lossy rendering of it.
fn screen_line(sys: &Pet, line: u16) -> Vec<u8> {
    let base = 0x8000 + line * 40;
    (base..base + 40).map(|a| sys.peek(a)).collect()
}

/// Boot a PET on the real ROM set, or `None` when they are not installed.
fn booted() -> Option<Pet> {
    let (kernal, basic, editor, charrom) = (
        rom("kernal.rom")?,
        rom("basic.rom")?,
        rom("editor.rom")?,
        rom("char.rom")?,
    );
    let mut sys = Pet::new(kernal, basic, editor, charrom, 40);
    for _ in 0..120 {
        sys.run_frame();
    }
    Some(sys)
}

fn screen(sys: &Pet) -> String {
    (0x8000u16..0x83E8)
        .map(|a| {
            let c = sys.peek(a) & 0x7F;
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
#[ignore = "FIXTURE: needs PET kernal/basic/editor/char ROMs — run with --ignored"]
fn types_a_basic_line_and_prints_the_result() {
    let (Some(kernal), Some(basic), Some(editor), Some(charrom)) = (
        rom("kernal.rom"),
        rom("basic.rom"),
        rom("editor.rom"),
        rom("char.rom"),
    ) else {
        panic!(
            "PET ROMs not found — set EMU198X_PET_ROMS or place \
             kernal.rom/basic.rom/editor.rom/char.rom at ~/.emu198x/roms/commodore-pet/"
        );
    };
    let mut sys = Pet::new(kernal, basic, editor, charrom, 40);
    for _ in 0..120 {
        sys.run_frame();
    }

    for key in [
        PetKey::P,
        PetKey::R,
        PetKey::I,
        PetKey::N,
        PetKey::T,
        PetKey::Num3,
        PetKey::Plus,
        PetKey::Num4,
        PetKey::Return,
    ] {
        tap(&mut sys, key);
    }
    for _ in 0..30 {
        sys.run_frame();
    }

    let screen = screen(&sys);
    assert!(
        screen.contains("PRINT3"),
        "expected the typed line to echo; got: {screen:?}"
    );
    assert!(
        screen.contains('7'),
        "expected BASIC to print the result 7; screen was: {screen:?}"
    );
}

/// The keypad minus. It was missing from the matrix entirely, so `-` was
/// the one arithmetic operator `type_string` could not reach (#1206); the
/// cell it belongs in is row 8 column 7, the keypad's bottom-right pair
/// with `=`. Typing a subtraction is the check that it landed right.
#[test]
#[ignore = "FIXTURE: needs PET kernal/basic/editor/char ROMs — run with --ignored"]
fn types_a_subtraction_through_the_keypad_minus() {
    let (Some(kernal), Some(basic), Some(editor), Some(charrom)) = (
        rom("kernal.rom"),
        rom("basic.rom"),
        rom("editor.rom"),
        rom("char.rom"),
    ) else {
        panic!(
            "PET ROMs not found — set EMU198X_PET_ROMS or place \
             kernal.rom/basic.rom/editor.rom/char.rom at ~/.emu198x/roms/commodore-pet/"
        );
    };
    let mut sys = Pet::new(kernal, basic, editor, charrom, 40);
    for _ in 0..120 {
        sys.run_frame();
    }

    for key in [
        PetKey::P,
        PetKey::R,
        PetKey::I,
        PetKey::N,
        PetKey::T,
        PetKey::Num9,
        PetKey::Minus,
        PetKey::Num4,
        PetKey::Return,
    ] {
        tap(&mut sys, key);
    }
    for _ in 0..30 {
        sys.run_frame();
    }

    let screen = screen(&sys);
    assert!(
        screen.contains("PRINT9-4"),
        "expected the minus to echo in the typed line; got: {screen:?}"
    );
    assert!(
        screen.split(' ').any(|tok| tok == "5"),
        "expected BASIC to print the result 5; screen was: {screen:?}"
    );
}

/// The bracket/arrow group, which the matrix had no cells for at all, so
/// `type_string` refused all five characters (#1206). Screen codes rather
/// than a rendered string: these are exactly the characters a lossy decode
/// would smear together.
#[test]
#[ignore = "FIXTURE: needs PET kernal/basic/editor/char ROMs — run with --ignored"]
fn types_the_bracket_and_arrow_keys() {
    let Some(mut sys) = booted() else {
        panic!("PET ROMs not found — set EMU198X_PET_ROMS");
    };
    for key in [
        PetKey::BracketLeft,
        PetKey::BracketRight,
        PetKey::Backslash,
        PetKey::UpArrow,
        PetKey::LeftArrow,
    ] {
        tap(&mut sys, key);
    }
    let line = screen_line(&sys, 5);
    assert_eq!(
        &line[..5],
        // [ ] \ ^ and the left-arrow glyph, in screen codes.
        &[0x1B, 0x1D, 0x1C, 0x1E, 0x1F],
        "expected the five keycaps to deposit their own glyphs; line was {line:?}"
    );
}

/// `(8, 2)` was mapped as `CursorRight`, so asking for cursor-right typed a
/// `]` instead of moving the cursor — and typing something is not an error,
/// so nothing surfaced it. Cursor right is `(0, 7)`.
#[test]
#[ignore = "FIXTURE: needs PET kernal/basic/editor/char ROMs — run with --ignored"]
fn cursor_right_moves_the_cursor_instead_of_typing() {
    let Some(mut sys) = booted() else {
        panic!("PET ROMs not found — set EMU198X_PET_ROMS");
    };
    for key in [PetKey::A, PetKey::B, PetKey::CursorRight, PetKey::C] {
        tap(&mut sys, key);
    }
    let line = screen_line(&sys, 5);
    assert_eq!(
        &line[..4],
        // A, B, the skipped-over space, then C — no glyph from the cursor key.
        &[0x01, 0x02, 0x20, 0x03],
        "expected cursor-right to skip a cell, not deposit one; line was {line:?}"
    );
}
