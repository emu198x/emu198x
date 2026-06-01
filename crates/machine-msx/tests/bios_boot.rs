//! MSX1 BIOS boot smoke.
//!
//! Loads a 32 KB MSX1 BIOS (either the real Microsoft/ASCII BIOS
//! from a TOSEC dump or the free C-BIOS replacement) and verifies
//! the boot screen renders a non-trivial framebuffer within 200
//! frames. Gated `#[ignore]` because the real BIOS is copyrighted
//! and the C-BIOS replacement is not shipped in-tree.
//!
//! Run with:
//! ```text
//! cargo test --release -p machine-msx \
//!     --test bios_boot -- --ignored --nocapture
//! ```
//!
//! BIOS source (first match wins):
//!   1. `EMU198X_MSX_BIOS` env var (full file path)
//!   2. `~/.emu198x/roms/microsoft-msx/msx.rom`
//!   3. `~/.emu198x/roms/microsoft-msx/cbios_main_msx1.rom`

use std::env;
use std::fs;
use std::path::PathBuf;

use machine_msx::{Msx, MsxRegion};

fn bios_path() -> Option<PathBuf> {
    if let Ok(path) = env::var("EMU198X_MSX_BIOS") {
        let p = PathBuf::from(path);
        if p.exists() {
            return Some(p);
        }
    }
    let home = env::var("HOME").ok()?;
    for name in ["msx.rom", "cbios_main_msx1.rom"] {
        let p = PathBuf::from(&home)
            .join(".emu198x/roms/microsoft-msx")
            .join(name);
        if p.exists() {
            return Some(p);
        }
    }
    None
}

#[test]
#[ignore = "needs a 32 KB MSX1 BIOS (real or C-BIOS) — run with --ignored"]
fn bios_boots_to_initial_screen() {
    let Some(path) = bios_path() else {
        panic!(
            "MSX1 BIOS not found — set EMU198X_MSX_BIOS or place msx.rom / \
             cbios_main_msx1.rom at ~/.emu198x/roms/microsoft-msx/"
        );
    };
    let bios = fs::read(&path).expect("read BIOS");
    assert_eq!(bios.len(), 32768, "BIOS must be exactly 32 KB");

    let mut sys = Msx::new(bios, MsxRegion::Ntsc);
    for _ in 0..200 {
        sys.run_frame();
    }

    let fb = sys.framebuffer();
    assert_eq!(
        fb.len(),
        (sys.framebuffer_width() * sys.framebuffer_height()) as usize
    );
    let mut colours = std::collections::HashSet::new();
    for &px in fb {
        colours.insert(px);
        if colours.len() >= 16 {
            break;
        }
    }
    assert!(
        colours.len() >= 2,
        "framebuffer should have >= 2 distinct colours; got {} (bios: {})",
        colours.len(),
        path.display()
    );
    let non_zero = fb.iter().filter(|&&px| px & 0x00FF_FFFF != 0).count();
    assert!(
        non_zero >= 1024,
        "boot screen should have >= 1024 non-backdrop pixels; got {non_zero}"
    );
}
