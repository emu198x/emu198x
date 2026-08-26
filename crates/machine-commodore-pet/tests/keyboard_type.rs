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
#[ignore = "needs PET kernal/basic/editor/char ROMs — run with --ignored"]
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
#[ignore = "needs PET kernal/basic/editor/char ROMs — run with --ignored"]
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
