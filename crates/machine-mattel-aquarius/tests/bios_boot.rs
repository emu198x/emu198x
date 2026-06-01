//! Mattel Aquarius BIOS boot smoke.
//!
//! Loads the 8 KB Microsoft Aquarius BASIC ROM from the user's local
//! ROM directory and verifies the boot screen renders a non-trivial
//! framebuffer within 200 frames. Gated `#[ignore]` because the BIOS
//! is copyrighted and not shipped in-tree.
//!
//! Run with:
//! ```text
//! cargo test --release -p machine-mattel-aquarius \
//!     --test bios_boot -- --ignored --nocapture
//! ```
//!
//! BIOS source (first match wins):
//!   1. `EMU198X_AQUARIUS_BIOS` env var (full file path)
//!   2. `~/.emu198x/roms/mattel-aquarius/aquarius.rom`

use std::env;
use std::fs;
use std::path::PathBuf;

use machine_mattel_aquarius::Aquarius;

fn bios_path() -> Option<PathBuf> {
    if let Ok(p) = env::var("EMU198X_AQUARIUS_BIOS") {
        let p = PathBuf::from(p);
        if p.exists() {
            return Some(p);
        }
    }
    let home = env::var("HOME").ok()?;
    let p = PathBuf::from(home).join(".emu198x/roms/mattel-aquarius/aquarius.rom");
    p.exists().then_some(p)
}

#[test]
#[ignore = "needs Aquarius BASIC ROM (8 KB) — run with --ignored"]
fn bios_boots_to_initial_screen() {
    let Some(path) = bios_path() else {
        panic!(
            "Aquarius BIOS not found — set EMU198X_AQUARIUS_BIOS or place aquarius.rom \
             at ~/.emu198x/roms/mattel-aquarius/"
        );
    };
    let bios = fs::read(&path).expect("read BIOS");
    assert_eq!(bios.len(), 0x2000, "BIOS must be exactly 8 KB");

    let mut sys = Aquarius::new(bios, 0);
    for _ in 0..200 {
        sys.run_frame();
    }

    let fb = sys.framebuffer();
    assert_eq!(fb.len(), 320 * 192);
    let mut colours = std::collections::HashSet::new();
    for &px in fb {
        colours.insert(px);
        if colours.len() >= 16 {
            break;
        }
    }
    // Aquarius cold boot uses a magenta-and-black palette with title
    // characters written to the centre of the screen.
    assert!(
        colours.len() >= 2,
        "framebuffer should have >= 2 distinct colours; got {} (bios: {})",
        colours.len(),
        path.display()
    );
    let non_zero = fb.iter().filter(|&&px| px & 0x00FF_FFFF != 0).count();
    assert!(
        non_zero >= 4096,
        "boot screen should have >= 4096 non-backdrop pixels; got {non_zero}"
    );
}
