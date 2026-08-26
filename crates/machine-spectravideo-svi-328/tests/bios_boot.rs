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
#[ignore = "FIXTURE: needs SVI-328 system ROM (32 KB) — run with --ignored"]
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
    for _ in 0..900 {
        sys.run_frame();
    }

    // The boot must reach SV-BASIC, not merely render something. The original
    // bug left the VDP display blanked forever: the vblank ISR reads the VDP
    // status at $85 to acknowledge the interrupt, but the I/O map pointed $85
    // at the keyboard, so the interrupt never cleared and the BIOS stalled
    // before turning the display on. A booted machine has the display enabled
    // (R1 bit 6 set) and the BASIC banner plus function-key strip on screen.
    let r1 = sys.vdp().registers()[1];
    assert_ne!(
        r1 & 0x40,
        0,
        "VDP display-enable (R1 bit 6) should be set after boot; R1={r1:#04x} (bios: {})",
        path.display()
    );

    let fb = sys.framebuffer();
    assert!(!fb.is_empty(), "framebuffer should be allocated");
    let mut counts = std::collections::HashMap::new();
    for &px in fb {
        *counts.entry(px).or_insert(0usize) += 1;
    }
    // SV-BASIC runs in the TMS9918 40-column TEXT mode, which is two colours
    // (ink and paper). The blue paper dominates; the banner and the
    // function-key strip across the bottom contribute a few thousand ink
    // pixels.
    assert!(
        counts.len() >= 2,
        "expected ink and paper; got {} colours",
        counts.len()
    );
    let paper = *counts.values().max().expect("non-empty framebuffer");
    let foreground = fb.len() - paper;
    assert!(
        foreground >= 1000,
        "expected the banner and key strip; got {foreground} ink pixels"
    );
}
