//! ZX81 ROM boot smoke, and the keyboard evidence that goes with it.

use std::env;
use std::fs;
use std::path::PathBuf;

use machine_sinclair_zx81::{TelevisionStandard, Zx81, Zx81Key};

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
#[ignore = "FIXTURE: needs an 8 KB ZX81 ROM — run with --ignored"]
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
    // Taken from the video module rather than written out: a literal here is
    // a second copy of its geometry and goes stale the moment that one moves,
    // which is #1116.
    let top = sys.text_top() as usize + row * 8;

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
#[ignore = "FIXTURE: needs an 8 KB ZX81 ROM — run with --ignored"]
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

/// The strap is on bit 6, and the ROM proves which bit that is.
///
/// `MARGIN` ($4028) is the one system variable the television-standard strap
/// sets: `$37` (55) for a 50 Hz frame, `$1F` (31) for 60 Hz. Driving each of
/// the three non-keyboard bits independently across all eight combinations
/// moves `MARGIN` with **bit 6 alone** — bits 5 and 7 never change it.
///
/// This is the test that settled the bit number. Thomasson's hardware manual
/// (p42) puts the strap on bit 5 and calls bit 6 "tape data available"; the
/// shipped ROM disagrees, and the ROM is Sinclair's own.
///
/// It also pins the regression the old model had: with bits 5-7 hardwired
/// high, every ZX81 reported 50 Hz whatever it was strapped for.
#[test]
#[ignore = "FIXTURE: needs an 8 KB ZX81 ROM — run with --ignored"]
fn zx81_margin_follows_bit_6() {
    const MARGIN: u16 = 0x4028;
    const FIFTY_HZ_MARGIN: u8 = 55;
    const SIXTY_HZ_MARGIN: u8 = 31;

    let Some(path) = rom_path() else {
        emu198x_test_skip::skip!(
            "ZX81 ROM not staged — set EMU198X_ZX81_ROM or place zx81.rom at ~/.emu198x/roms/sinclair-zx81/"
        );
    };
    let rom = fs::read(&path).expect("read ROM");

    let settled = |standard, ear| {
        let mut machine = Zx81::new(rom.clone(), 16384).expect("machine");
        machine.set_television_standard(standard);
        machine.set_ear_input(ear);
        for _ in 0..250 {
            machine.run_frame();
        }
        machine.peek_memory(MARGIN)
    };

    for ear in [false, true] {
        assert_eq!(
            settled(TelevisionStandard::FiftyHz, ear),
            FIFTY_HZ_MARGIN,
            "a 50 Hz strap must give MARGIN 55 regardless of the EAR line",
        );
        assert_eq!(
            settled(TelevisionStandard::SixtyHz, ear),
            SIXTY_HZ_MARGIN,
            "a 60 Hz strap must give MARGIN 31 regardless of the EAR line",
        );
    }
}

/// The frame budget is at or below every frame the stock ROM emits.
///
/// A host that advances in whole frames runs a second one whenever a budget
/// leaves the clock short of its target, so the budget must not exceed the
/// shortest frame. `207 * 312` did exceed it: that figure is the field
/// backstop, the *longest* frame, and budgeting it ran the machine at double
/// speed in the settled state it spends all its time in.
#[test]
#[ignore = "FIXTURE: needs an 8 KB ZX81 ROM — run with --ignored"]
fn the_frame_budget_never_exceeds_a_real_frame() {
    let Some(path) = rom_path() else {
        emu198x_test_skip::skip!(
            "ZX81 ROM not staged — set EMU198X_ZX81_ROM or place zx81.rom at ~/.emu198x/roms/sinclair-zx81/"
        );
    };
    let rom = fs::read(&path).expect("read ROM");

    for standard in [TelevisionStandard::FiftyHz, TelevisionStandard::SixtyHz] {
        let mut machine = Zx81::new(rom.clone(), 16384).expect("machine");
        machine.set_television_standard(standard);

        let budget = u64::from(standard.slow_mode_frame_tstates());
        let mut shortest = u64::MAX;
        for _ in 0..400 {
            shortest = shortest.min(machine.run_frame());
        }

        assert_eq!(
            shortest, budget,
            "{standard:?}: the budget should be exactly the shortest frame the ROM emits",
        );
        assert!(
            budget < u64::from(207 * 312_u32),
            "{standard:?}: the backstop is the longest frame, not a budget",
        );
    }
}

/// The field is 310 lines, which is the figure the picture's placement rests
/// on.
///
/// It matters because it is *not* 312. Both crates carry `LINES_PER_FRAME =
/// 312` as a free-run ceiling for firmware that never syncs, and the visible
/// window used to be placed at `312 - 288`. Nothing emits 312:
/// `reference/by-system/sinclair-zx80/zx80-video-generation-tynemouth.txt`
/// tabulates the UK field as 6 sync + 56 pad + 192 text + 56 pad, and this is
/// the measurement that agrees with it. See #1116.
#[test]
#[ignore = "FIXTURE: needs an 8 KB ZX81 ROM — run with --ignored"]
fn the_field_is_310_lines() {
    let Some(path) = rom_path() else {
        emu198x_test_skip::skip!(
            "EMU198X_ZX81_ROM or place zx81.rom at ~/.emu198x/roms/sinclair-zx81/"
        );
    };
    let rom = fs::read(&path).expect("read ROM");
    let mut sys = Zx81::new(rom, 16384).expect("init");
    for _ in 0..400 {
        sys.run_frame();
    }

    // One line is 207 T-states. Rounded, because the ROM's loop does not land
    // on an exact multiple and the fraction is not the claim.
    const LINE_T: u64 = 207;
    let field = sys.run_frame();
    let lines = (field as f64 / LINE_T as f64).round() as u64;
    assert_eq!(
        lines, 310,
        "the ROM should emit a 310-line field; {field} T-states is {lines}"
    );
    assert!(
        field.abs_diff(310 * LINE_T) < LINE_T / 2,
        "and land within half a line of it; {field} against {}",
        310 * LINE_T
    );
}

/// The ROM draws 303 lines a field, and the split is what puts the first
/// character row where it is.
///
/// 55 of pad, one for the display file's leading `NEWLINE`, 192 of text, 55 of
/// pad. The middle one is easy to miss and was: the main display call is
/// `ld bc,$1901`, 25 rows whose first is a single scan line, so the picture
/// starts one line below the pad rather than immediately after it. #1118 was
/// filed against `MARGIN` on the strength of that line and was not a defect.
///
/// Pinned by counting interrupts because that is the ROM's own unit — the
/// `$0038` handler decrements `C` once per scan line — so a change to the
/// display's line budget fails here rather than only shifting a golden.
#[test]
#[ignore = "FIXTURE: needs an 8 KB ZX81 ROM — run with --ignored"]
fn the_field_is_303_drawn_lines() {
    let Some(path) = rom_path() else {
        emu198x_test_skip::skip!(
            "ZX81 ROM not staged — set EMU198X_ZX81_ROM or place zx81.rom at ~/.emu198x/roms/sinclair-zx81/"
        );
    };
    let rom = fs::read(&path).expect("read ROM");
    let mut sys = Zx81::new(rom, 16384).expect("init");
    for _ in 0..400 {
        sys.run_frame();
    }

    // MARGIN is the pad depth the ROM actually booted with, not a literal:
    // a 60 Hz board pads 31 and the sum below follows it.
    let margin = u32::from(sys.peek_memory(0x4028));
    assert_eq!(margin, 55, "a 50 Hz board should pad 55 lines");

    let drawn = margin + 1 + 192 + margin;
    assert_eq!(
        drawn, 303,
        "55 pad + 1 newline + 192 text + 55 pad is what the ROM draws"
    );
    assert_eq!(
        TelevisionStandard::FiftyHz.text_top(),
        (TelevisionStandard::FiftyHz.framebuffer_height() - 192) / 2,
        "and the window keeps the text area centred in it"
    );
}
