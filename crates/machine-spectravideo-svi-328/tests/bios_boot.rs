//! Spectravideo SVI-328 BIOS boot smoke.
//!
//! Loads the 32 KB SVI-328 system ROM (BASIC + OS) from the user's
//! local ROM directory and verifies the boot screen renders a
//! non-trivial framebuffer within 300 frames. Gated `#[ignore]`
//! because the system ROM is copyrighted and not shipped in-tree.
//!
//! Run with:
//! ```text
//! cargo test --release -p machine-spectravideo-svi-328 \
//!     --test bios_boot -- --ignored --nocapture
//! ```
//!
//! BIOS source (first match wins):
//!   1. `EMU198X_SVI_328_BIOS` env var (full file path)
//!   2. `~/.emu198x/roms/spectravideo-svi-328/svi-328.rom`

use std::env;
use std::fs;
use std::path::PathBuf;

use machine_spectravideo_svi_328::{Svi328, SviRegion};

fn bios_path() -> Option<PathBuf> {
    if let Ok(p) = env::var("EMU198X_SVI_328_BIOS") {
        let p = PathBuf::from(p);
        if p.exists() {
            return Some(p);
        }
    }
    let home = env::var("HOME").ok()?;
    let p = PathBuf::from(home).join(".emu198x/roms/spectravideo-svi-328/svi-328.rom");
    p.exists().then_some(p)
}

#[test]
#[ignore = "needs SVI-328 system ROM (32 KB) — run with --ignored"]
fn bios_boots_to_initial_screen() {
    let Some(path) = bios_path() else {
        panic!(
            "SVI-328 BIOS not found — set EMU198X_SVI_328_BIOS or place svi-328.rom \
             at ~/.emu198x/roms/spectravideo-svi-328/"
        );
    };
    let bios = fs::read(&path).expect("read BIOS");
    assert_eq!(bios.len(), 0x8000, "BIOS must be exactly 32 KB");

    let mut sys = Svi328::new(bios, SviRegion::Ntsc);
    for _ in 0..300 {
        sys.run_frame();
    }

    let fb = sys.framebuffer();
    assert_eq!(fb.len(), 256 * 192);
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
