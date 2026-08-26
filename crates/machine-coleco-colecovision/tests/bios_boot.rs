//! ColecoVision BIOS boot smoke.
//!
//! Loads the canonical 1982 ColecoVision BIOS from the user's ROM
//! directory and verifies the title screen renders to a non-trivial
//! framebuffer within 200 frames. Gated `#[ignore]` because the BIOS
//! is copyrighted and not shipped with the repo.
//!
//! Run with:
//! ```text
//! cargo test --release -p machine-coleco-colecovision \
//!     --test bios_boot -- --ignored --nocapture
//! ```
//!
//! ROM directory convention (first match wins):
//!   1. `EMU198X_COLECOVISION_BIOS` env var (full file path)
//!   2. `~/.emu198x/roms/coleco-colecovision/colecovision.rom`

use std::env;
use std::fs;
use std::path::PathBuf;

use machine_coleco_colecovision::{ColecoVision, CvRegion};

fn bios_path() -> Option<PathBuf> {
    if let Ok(path) = env::var("EMU198X_COLECOVISION_BIOS") {
        let p = PathBuf::from(path);
        if p.exists() {
            return Some(p);
        }
    }
    let home = env::var("HOME").ok()?;
    let p = PathBuf::from(home).join(".emu198x/roms/coleco-colecovision/colecovision.rom");
    p.exists().then_some(p)
}

#[test]
#[ignore = "FIXTURE: needs ColecoVision BIOS — run with --ignored"]
fn bios_boots_to_title_screen() {
    let Some(path) = bios_path() else {
        panic!(
            "ColecoVision BIOS not found — set EMU198X_COLECOVISION_BIOS or place at \
             ~/.emu198x/roms/coleco-colecovision/colecovision.rom"
        );
    };
    let bios = fs::read(&path).expect("read BIOS");
    assert_eq!(bios.len(), 8192, "BIOS must be exactly 8 KB");

    let mut cv = ColecoVision::new(bios, vec![], CvRegion::Ntsc);
    for _ in 0..200 {
        cv.run_frame();
    }

    // The title screen draws "COLECOVISION", "TURN GAME OFF", etc. on
    // a black backdrop with multiple non-backdrop colours. After 200
    // frames the framebuffer must contain a meaningful spread of
    // distinct colours — the boot does not stay on a single solid
    // colour. We sample for >= 4 distinct colours including at least
    // one non-zero (non-backdrop) value.
    let fb = cv.framebuffer();
    assert_eq!(
        fb.len(),
        (cv.framebuffer_width() * cv.framebuffer_height()) as usize,
        "framebuffer size matches reported dimensions"
    );

    let mut colours = std::collections::HashSet::new();
    for &px in fb {
        colours.insert(px);
        if colours.len() >= 16 {
            break;
        }
    }
    assert!(
        colours.len() >= 4,
        "title screen should have >= 4 distinct colours; got {}",
        colours.len()
    );
    let non_zero = fb.iter().filter(|&&px| px & 0x00FF_FFFF != 0).count();
    assert!(
        non_zero >= 1024,
        "title screen should have >= 1024 non-backdrop pixels; got {non_zero}"
    );
}
