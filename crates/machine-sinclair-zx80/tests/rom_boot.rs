//! ZX80 ROM boot smoke.

use std::env;
use std::fs;
use std::path::PathBuf;

use machine_sinclair_zx80::Zx80;

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
#[test]
#[ignore = "needs a 4 KB ZX80 ROM — run with --ignored"]
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
    assert!(
        ink > 0,
        "but the cursor should be drawn — an all-paper frame means nothing rendered"
    );
}
