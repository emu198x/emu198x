//! The ZX80's keyword layout, asserted against Sinclair's own ROM.
//!
//! At the `K` cursor a letter key inserts a whole BASIC word, and **the ZX80's
//! arrangement is not the ZX81's** — `PRINT` is on `O`, and `L`, `M`, `P` and
//! `Z` carry no keyword at all. The ZX81 layout is the one that gets
//! remembered, so this is easy to get wrong from memory and hard to notice
//! when you do: press `P` expecting `PRINT` and the machine answers `?` with
//! an inverse `S`, which reads like a broken keyboard rather than a correct
//! refusal.
//!
//! The table is documented at
//! `reference/by-system/sinclair-zx80/zx80-keyword-entry-reference.md`, where
//! `PRINT`/`O`, `NEW`/`Q` and `IF`/`U` are corroborated in prose by *The ZX80
//! Companion*. This asserts the whole of it, so a regression in the CPU, the
//! keyboard matrix or the character generator shows up as a wrong word rather
//! than as a screen nobody reads.
//!
//! Reading method matters. This decodes the framebuffer against the character
//! bitmaps in the ROM under test, rather than walking memory for the display
//! file: an earlier probe that scanned for the first `$76` found the system
//! variables instead and reported `U` as producing nothing at all.

use std::env;
use std::fs;
use std::path::PathBuf;

use machine_sinclair_zx80::{Zx80, Zx80Key};

/// ZX80 character set, codes 0..=63. Graphics codes are irrelevant here and
/// are named rather than drawn.
const CHARS: [&str; 64] = [
    " ", "g1", "g2", "g3", "g4", "g5", "g6", "g7", "g8", "g9", "ga", "\"", "£", "$", ":", "?", "(",
    ")", "-", "+", "*", "/", "=", ">", "<", ";", ",", ".", "0", "1", "2", "3", "4", "5", "6", "7",
    "8", "9", "A", "B", "C", "D", "E", "F", "G", "H", "I", "J", "K", "L", "M", "N", "O", "P", "Q",
    "R", "S", "T", "U", "V", "W", "X", "Y", "Z",
];

/// The visible 32x24 character area within the 320x240 framebuffer.
const ORIGIN_X: usize = 32;
/// Framebuffer row the text area starts on.
///
/// Was a literal 24, fitted to a 240-line window and left behind when the
/// window became the 288 a set shows. Taken from the video module now, so it
/// cannot drift again — see #1116.
const ORIGIN_Y: usize = machine_sinclair_zx80::TelevisionStandard::FiftyHz.text_top() as usize;
const INK: u32 = 0xFF00_0000;

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

/// Decodes one screen row to text by matching each cell against the ROM's own
/// character bitmaps at `I * 256 + code * 8`, in normal and inverse form.
fn read_row(machine: &Zx80, rom: &[u8], row: usize) -> String {
    let frame = machine.framebuffer();
    let base = usize::from(machine.cpu().regs.i) << 8;
    let mut text = String::new();
    for column in 0..32usize {
        let mut cell = [0u8; 8];
        for (y, bits) in cell.iter_mut().enumerate() {
            for x in 0..8usize {
                let px = ORIGIN_X + column * 8 + x;
                let py = ORIGIN_Y + row * 8 + y;
                if frame[py * 320 + px] == INK {
                    *bits |= 0x80 >> x;
                }
            }
        }
        let glyph = (0..64usize).find_map(|code| {
            let bitmap = &rom[base + code * 8..base + code * 8 + 8];
            if cell[..] == bitmap[..] {
                Some(CHARS[code].to_owned())
            } else if cell.iter().zip(bitmap).all(|(a, b)| *a == !*b) {
                Some(format!("[{}]", CHARS[code]))
            } else {
                None
            }
        });
        text.push_str(&glyph.unwrap_or_else(|| "<?>".to_owned()));
    }
    text.trim_end().to_owned()
}

/// Boots, presses one key at the `K` cursor, and returns the input line.
fn press_at_k_cursor(rom: &[u8], key: Zx80Key) -> String {
    let mut machine = Zx80::new(rom.to_vec(), 16 * 1024).expect("init");
    for _ in 0..250 {
        machine.run_frame();
    }
    machine.press_key(key);
    for _ in 0..6 {
        machine.run_frame();
    }
    machine.release_key(key);
    for _ in 0..80 {
        machine.run_frame();
    }
    // Row 23 is the input line: the bottom row of the 24-row display.
    read_row(&machine, rom, 23)
}

/// `[L]` is the literal cursor, `[S]` the syntax-error marker. Both are
/// expected and neither is part of the keyword.
fn keyword(line: &str) -> String {
    line.replace("[L]", "").replace("[S]", "").to_owned()
}

#[test]
#[ignore = "needs a 4 KB ZX80 ROM — run with --ignored"]
fn keyword_layout_is_the_zx80s_own_not_the_zx81s() {
    let Some(path) = rom_path() else {
        emu198x_test_skip::skip!(
            "ZX80 ROM not staged — set EMU198X_ZX80_ROM or place zx80.rom at ~/.emu198x/roms/sinclair-zx80/"
        );
    };
    let rom = fs::read(&path).expect("read ROM");
    assert_eq!(rom.len(), 0x1000, "ROM must be exactly a 4 KB ZX80");

    // `GO TO` and `GO SUB` are two words, and `RANDOMISE` takes an S: these
    // are the ROM's spellings, not a transcription choice. Every keyword is
    // followed by a trailing space in the edit line.
    let expected: [(Zx80Key, char, &str); 26] = [
        (Zx80Key::A, 'A', "LIST "),
        (Zx80Key::B, 'B', "RETURN "),
        (Zx80Key::C, 'C', "CLS "),
        (Zx80Key::D, 'D', "DIM "),
        (Zx80Key::E, 'E', "SAVE "),
        (Zx80Key::F, 'F', "FOR "),
        (Zx80Key::G, 'G', "GO TO "),
        (Zx80Key::H, 'H', "POKE "),
        (Zx80Key::I, 'I', "INPUT "),
        (Zx80Key::J, 'J', "RANDOMISE "),
        (Zx80Key::K, 'K', "LET "),
        (Zx80Key::L, 'L', "?"),
        (Zx80Key::M, 'M', "?"),
        (Zx80Key::N, 'N', "NEXT "),
        (Zx80Key::O, 'O', "PRINT "),
        (Zx80Key::P, 'P', "?"),
        (Zx80Key::Q, 'Q', "NEW "),
        (Zx80Key::R, 'R', "RUN "),
        (Zx80Key::S, 'S', "STOP "),
        (Zx80Key::T, 'T', "CONTINUE "),
        (Zx80Key::U, 'U', "IF "),
        (Zx80Key::V, 'V', "GO SUB "),
        (Zx80Key::W, 'W', "LOAD "),
        (Zx80Key::X, 'X', "CLEAR "),
        (Zx80Key::Y, 'Y', "REM "),
        (Zx80Key::Z, 'Z', "?"),
    ];

    let mut wrong = Vec::new();
    for (key, label, want) in expected {
        let line = press_at_k_cursor(&rom, key);
        let got = keyword(&line);
        if got.trim_end() != want.trim_end() {
            wrong.push(format!(
                "{label}: expected {want:?}, got {got:?} (line {line:?})"
            ));
        }
    }
    assert!(
        wrong.is_empty(),
        "the ZX80's keyword layout has changed:\n  {}",
        wrong.join("\n  ")
    );
}

/// The three mappings *The ZX80 Companion* states in prose, called out
/// separately because they are the ones with a second source behind them.
#[test]
#[ignore = "needs a 4 KB ZX80 ROM — run with --ignored"]
fn the_companion_s_three_documented_keys_match() {
    let Some(path) = rom_path() else {
        emu198x_test_skip::skip!("ZX80 ROM not staged");
    };
    let rom = fs::read(&path).expect("read ROM");

    // "PRINT is entered by pressing the O key, rather than P-R-1-N-T"
    assert_eq!(keyword(&press_at_k_cursor(&rom, Zx80Key::O)), "PRINT ");
    // "press the NEW key (Q)"
    assert_eq!(keyword(&press_at_k_cursor(&rom, Zx80Key::Q)), "NEW ");
    // "if key U is pressed when the cursor is 'K' then IF will be displayed"
    assert_eq!(keyword(&press_at_k_cursor(&rom, Zx80Key::U)), "IF ");
}

/// The other half of the same rule: at the `L` cursor a key is just its
/// letter. Typing `PRINT 42` is `O`, `4`, `2` — the digits land as digits
/// because the keyword switched the cursor to `L`.
#[test]
#[ignore = "needs a 4 KB ZX80 ROM — run with --ignored"]
fn a_keyword_switches_to_literal_entry_and_the_line_runs() {
    let Some(path) = rom_path() else {
        emu198x_test_skip::skip!("ZX80 ROM not staged");
    };
    let rom = fs::read(&path).expect("read ROM");

    let mut machine = Zx80::new(rom.clone(), 16 * 1024).expect("init");
    for _ in 0..250 {
        machine.run_frame();
    }
    for key in [Zx80Key::O, Zx80Key::N4, Zx80Key::N2, Zx80Key::Newline] {
        machine.press_key(key);
        for _ in 0..6 {
            machine.run_frame();
        }
        machine.release_key(key);
        for _ in 0..80 {
            machine.run_frame();
        }
    }

    // `PRINT 42` executed: the answer is at the top of the screen, where the
    // ZX80 puts program output.
    let printed = read_row(&machine, &rom, 0);
    assert_eq!(
        printed, "42",
        "PRINT 42 should print 42 on the top line; got {printed:?}"
    );
}
