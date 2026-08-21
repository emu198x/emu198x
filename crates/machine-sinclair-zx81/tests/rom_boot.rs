//! ZX81 ROM boot smoke, and the keyboard evidence that goes with it.

use std::env;
use std::fs;
use std::path::PathBuf;

use machine_sinclair_zx81::{Zx81, Zx81Key};

fn rom_path() -> Option<PathBuf> {
    if let Ok(p) = env::var("EMU198X_ZX81_ROM") {
        let p = PathBuf::from(p);
        if p.exists() {
            return Some(p);
        }
    }
    let home = env::var("HOME").ok()?;
    let p = PathBuf::from(home).join(".emu198x/roms/sinclair-zx81/zx81.rom");
    p.exists().then_some(p)
}

/// The real ROM reaches its power-on screen.
///
/// This used to assert only `frame_count >= 200` — that the machine had not
/// panicked. It could not tell a working machine from one rendering
/// executable bytes as glyphs, which is exactly what #1030 turned out to be
/// doing: before the fix this ROM produced a frame **39% black** with the
/// character address hardcoded to `$0000`.
///
/// Three assertions, because each alone is satisfiable by a failure:
///
/// - `I` is `0x1E` — the firmware pointed the character address at its
///   own character set, which is the mechanism #1030 restored
/// - the frame is overwhelmingly paper — garbage glyphs are not
/// - but *some* ink is present, so a framebuffer nobody drew into (the ULA
///   clears to white) fails too
#[test]
#[ignore = "needs an 8 KB ZX81 ROM — run with --ignored"]
fn rom_boots_to_its_power_on_screen() {
    let Some(path) = rom_path() else {
        emu198x_test_skip::skip!(
            "ZX81 ROM not staged — set EMU198X_ZX81_ROM or place zx81.rom at ~/.emu198x/roms/sinclair-zx81/"
        );
    };
    let rom = fs::read(&path).expect("read ROM");
    assert_eq!(rom.len(), 0x2000, "ROM must be exactly an 8 KB ZX81");

    let mut sys = Zx81::new(rom, 16384).expect("init");
    for _ in 0..200 {
        sys.run_frame();
    }
    assert!(sys.frame_count() >= 200);

    assert_eq!(
        sys.cpu().regs.i,
        0x1E,
        "the firmware should point I at its character set"
    );

    let frame = sys.framebuffer();
    let ink = frame.iter().filter(|&&pixel| pixel == 0xFF00_0000).count();
    assert!(
        ink * 100 / frame.len() < 5,
        "the power-on screen is almost all paper; {ink} of {} pixels are ink, \
         which is what rendering code bytes as glyphs looks like",
        frame.len()
    );
    assert!(
        ink > 0,
        "but the cursor should be drawn — an all-paper frame means nothing rendered"
    );
}

/// Read a row of the screen back as characters.
///
/// Decoded against the ROM's own bitmaps at `(I & $FE) << 8 | code << 3`
/// rather than walked from the display file. #1040 is why: a scan for `$76`
/// found the system variables instead of the picture, and the display file is
/// not the display in any case — since #1032 the picture is whatever the CPU
/// put on the bus, so reading it back off the screen is the only honest way
/// to ask what is on it.
fn read_row(sys: &Zx81, row: usize) -> String {
    let fb = sys.framebuffer();
    let width = sys.framebuffer_width() as usize;
    let i = u16::from(sys.cpu().regs.i);
    let top = 32 + row * 8;

    (0..32)
        .map(|col| {
            let left = 32 + col * 8;
            let cell: Vec<u8> = (0..8)
                .map(|r| {
                    (0..8).fold(0u8, |b, bit| {
                        if fb[(top + r) * width + left + bit] == 0xFF00_0000 {
                            b | (0x80 >> bit)
                        } else {
                            b
                        }
                    })
                })
                .collect();
            (0..64u16)
                .find_map(|code| {
                    let base = (i & 0xFE) << 8 | code << 3;
                    let glyph: Vec<u8> = (0..8).map(|k| sys.peek_memory(base + k)).collect();
                    let inverse: Vec<u8> = glyph.iter().map(|b| !b).collect();
                    (glyph == cell || inverse == cell).then(|| character(code as u8))
                })
                .unwrap_or('?')
        })
        .collect()
}

/// The ZX81's character set, as far as this needs it.
fn character(code: u8) -> char {
    match code {
        0 => ' ',
        28..=37 => (b'0' + (code - 28)) as char,
        38..=63 => (b'A' + (code - 38)) as char,
        _ => '.',
    }
}

/// #1041: the ZX81 ignored every key.
///
/// It was downstream of #1032, and only an end-to-end test could have caught
/// it. Nothing was wrong with the keyboard code — `read_keyboard` and the
/// matrix have had unit tests throughout, and both passed. What was wrong is
/// that the ROM never reached the state where it processes input, because the
/// display was drawn for it at frame boundaries instead of by the CPU
/// executing the display file. A machine that is not really running its
/// display loop is not really running its editor either.
///
/// Against the commit before that fix this test reads `K` for all
/// twenty-six letters; against the one after, every one of them types its
/// keyword.
///
/// A minute to run: twenty-six machines booted from cold, because each
/// keyword needs a fresh editor. It is opt-in twice over — `--ignored`, and a
/// ROM that cannot be staged in CI — so the time is spent only by someone who
/// asked for it.
#[test]
#[ignore = "needs an 8 KB ZX81 ROM — run with --ignored"]
fn every_letter_types_its_keyword() {
    let Some(path) = rom_path() else {
        emu198x_test_skip::skip!(
            "ZX81 ROM not staged — set EMU198X_ZX81_ROM or place zx81.rom at ~/.emu198x/roms/sinclair-zx81/"
        );
    };
    let rom = fs::read(&path).expect("read ROM");

    // The K cursor offers a keyword per letter. This is the ZX81's layout,
    // read off its own ROM rather than transcribed from a manual.
    let expected = [
        (Zx81Key::A, "NEW"),
        (Zx81Key::B, "SCROLL"),
        (Zx81Key::C, "CONT"),
        (Zx81Key::D, "DIM"),
        (Zx81Key::E, "REM"),
        (Zx81Key::F, "FOR"),
        (Zx81Key::G, "GOTO"),
        (Zx81Key::H, "GOSUB"),
        (Zx81Key::I, "INPUT"),
        (Zx81Key::J, "LOAD"),
        (Zx81Key::K, "LIST"),
        (Zx81Key::L, "LET"),
        (Zx81Key::M, "PAUSE"),
        (Zx81Key::N, "NEXT"),
        (Zx81Key::O, "POKE"),
        (Zx81Key::P, "PRINT"),
        (Zx81Key::Q, "PLOT"),
        (Zx81Key::R, "RUN"),
        (Zx81Key::S, "SAVE"),
        (Zx81Key::T, "RAND"),
        (Zx81Key::U, "IF"),
        (Zx81Key::V, "CLS"),
        (Zx81Key::W, "UNPLOT"),
        (Zx81Key::X, "CLEAR"),
        (Zx81Key::Y, "RETURN"),
        (Zx81Key::Z, "COPY"),
    ];

    for (key, keyword) in expected {
        let mut sys = Zx81::new(rom.clone(), 16384).expect("init");
        for _ in 0..400 {
            sys.run_frame();
        }
        assert_eq!(
            read_row(&sys, 23).trim_end(),
            "K",
            "the machine should be at the K cursor before {keyword} is typed"
        );

        sys.press_key(key);
        for _ in 0..25 {
            sys.run_frame();
        }
        sys.release_key(key);
        for _ in 0..120 {
            sys.run_frame();
        }

        // The keyword, then the cursor, which has become L for letter mode.
        assert_eq!(
            read_row(&sys, 23).trim_end(),
            format!("{keyword} L"),
            "{keyword} should have been typed"
        );
    }
}
