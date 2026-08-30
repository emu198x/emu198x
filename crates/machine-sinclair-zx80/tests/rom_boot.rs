//! ZX80 ROM boot smoke.

use std::env;
use std::fs;
use std::path::PathBuf;

use machine_sinclair_zx80::{FB_WIDTH, Zx80};

fn rom_path() -> Option<PathBuf> {
    if let Ok(p) = env::var("EMU198X_ZX80_ROM") {
        let p = PathBuf::from(p);
        if p.exists() {
            return Some(p);
        }
    }
    let home = env::var("HOME").ok()?;
    let p = PathBuf::from(home).join(".emu198x/roms/sinclair-zx80/zx80.rom");
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
/// - `I` is `0x0E` — the firmware pointed the character address at its
///   own character set, which is the mechanism #1030 restored
/// - the frame is overwhelmingly paper — garbage glyphs are not
/// - but *some* ink is present, so a framebuffer nobody drew into (the ULA
///   clears to white) fails too
///
/// The cursor's position is the fourth assertion, and it is the one that
/// caught #1116: this test was `#[ignore]`d, so it went on asserting a row
/// the picture had left. The expectation now derives from the ROM's own
/// layout rather than from wherever the model happens to draw — see
/// [`machine_sinclair_zx80::video`]'s `FIRST_VISIBLE_LINE`.
/// Not `#[ignore]`d: it skips when no ROM is staged, which is what #295 asks
/// for and what `golden_frame.rs` already does. Ignoring it is what stopped
/// it running on the machines that *do* have the ROM.
#[test]
fn rom_boots_to_its_power_on_screen() {
    let Some(path) = rom_path() else {
        emu198x_test_skip::skip!(
            "ZX80 ROM not staged — set EMU198X_ZX80_ROM or place zx80.rom at ~/.emu198x/roms/sinclair-zx80/"
        );
    };
    let rom = fs::read(&path).expect("read ROM");
    assert_eq!(rom.len(), 0x1000, "ROM must be exactly a 4 KB ZX80");

    let mut sys = Zx80::new(rom, 16384).expect("init");
    for _ in 0..200 {
        sys.run_frame();
    }
    assert!(sys.frame_count() >= 200);

    assert_eq!(
        sys.cpu().regs.i,
        0x0E,
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

    // The cursor, and only the cursor. A ZX80 powers on to a blank screen
    // with an inverse `K` on the input line at the bottom left, so the ink
    // is one 8x8 cell, and the input line is the text area's last row.
    //
    // The text area's vertical position follows the ROM's pad; horizontally
    // the full-line MAME capture puts the ZX80's first text pixel at x=80.
    let row_23_top = sys.text_top() as usize + 23 * 8;
    let w = FB_WIDTH as usize;
    let ink_at = |x: usize, y: usize| frame[y * w + x] == 0xFF00_0000;
    let (mut min_x, mut max_x, mut min_y, mut max_y) = (usize::MAX, 0, usize::MAX, 0);
    for y in 0..sys.framebuffer_height() as usize {
        for x in 0..w {
            if ink_at(x, y) {
                min_x = min_x.min(x);
                max_x = max_x.max(x);
                min_y = min_y.min(y);
                max_y = max_y.max(y);
            }
        }
    }
    assert_eq!(
        (min_x, max_x, min_y, max_y),
        (80, 87, row_23_top, row_23_top + 7),
        "the cursor should be a single 8x8 cell at the bottom left of the \
         display area; anything wider means the character generator ran on \
         past the row's NEWLINE"
    );

    // Inverse video: the cell is mostly ink with the letter cut out of it.
    let cell_ink = (row_23_top..row_23_top + 8)
        .flat_map(|y| (80..88).map(move |x| (x, y)))
        .filter(|&(x, y)| ink_at(x, y))
        .count();
    assert!(
        (40..64).contains(&cell_ink),
        "an inverse `K` is a solid block with a letter knocked out of it, so \
         most of the 64 pixels are ink but not all; found {cell_ink}"
    );
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
fn the_field_is_310_lines() {
    let Some(path) = rom_path() else {
        emu198x_test_skip::skip!(
            "EMU198X_ZX80_ROM or place zx80.rom at ~/.emu198x/roms/sinclair-zx80/"
        );
    };
    let rom = fs::read(&path).expect("read ROM");
    let mut sys = Zx80::new(rom, 16384).expect("init");
    for _ in 0..200 {
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
