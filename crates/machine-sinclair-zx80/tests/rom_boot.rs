//! ZX80 ROM boot smoke.

use std::env;
use std::fs;
use std::path::PathBuf;

use machine_sinclair_zx80::{FB_HEIGHT, FB_WIDTH, Zx80};

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
/// ⚠ Still `#[ignore]`d, and #295 asks for the opposite.
///
/// Un-ignoring it fails: the cursor's bounding box is `(32, 39, 216, 223)`
/// against the `(32, 39, 208, 215)` asserted below — one character row lower.
/// The expectation was written in `99df0b7b` and passed then; three video
/// commits have landed since, including `fcaa04fd` taking the framebuffer
/// from 240 lines to the 288 a PAL set shows. The picture moved and nothing
/// noticed, because this test never ran.
///
/// Which row is *right* is exactly what #295's reference oracle is for, so
/// the expectation is left alone rather than updated to match whatever the
/// model currently does. `golden_frame.rs` pins the present output as a
/// baseline meanwhile.
#[test]
#[ignore = "needs a 4 KB ZX80 ROM, and currently fails — see the note above"]
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
    // is one 8x8 cell — the display area starts 24 rows down and 32 pixels
    // in, and the input line is its last row.
    let w = FB_WIDTH as usize;
    let ink_at = |x: usize, y: usize| frame[y * w + x] == 0xFF00_0000;
    let (mut min_x, mut max_x, mut min_y, mut max_y) = (usize::MAX, 0, usize::MAX, 0);
    for y in 0..FB_HEIGHT as usize {
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
        (32, 39, 208, 215),
        "the cursor should be a single 8x8 cell at the bottom left of the \
         display area; anything wider means the character generator ran on \
         past the row's NEWLINE"
    );

    // Inverse video: the cell is mostly ink with the letter cut out of it.
    let cell_ink = (208..216)
        .flat_map(|y| (32..40).map(move |x| (x, y)))
        .filter(|&(x, y)| ink_at(x, y))
        .count();
    assert!(
        (40..64).contains(&cell_ink),
        "an inverse `K` is a solid block with a letter knocked out of it, so \
         most of the 64 pixels are ink but not all; found {cell_ink}"
    );
}
