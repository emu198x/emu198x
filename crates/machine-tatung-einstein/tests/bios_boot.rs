//! Tatung Einstein TC-01 BIOS boot smoke.

use std::env;
use std::fs;
use std::path::PathBuf;

use machine_tatung_einstein::{Einstein, EinsteinRegion};

fn bios_path() -> Option<PathBuf> {
    if let Ok(p) = env::var("EMU198X_EINSTEIN_BIOS") {
        let p = PathBuf::from(p);
        if p.exists() {
            return Some(p);
        }
    }
    let home = env::var("HOME").ok()?;
    let p = PathBuf::from(home).join(".emu198x/roms/tatung-einstein/einstein.rom");
    p.exists().then_some(p)
}

#[test]
#[ignore = "FIXTURE: needs Tatung Einstein X-TAL MOS ROM (8 KB) — run with --ignored"]
fn bios_boots_to_mos_prompt() {
    let Some(path) = bios_path() else {
        panic!(
            "Einstein BIOS not found — set EMU198X_EINSTEIN_BIOS or place einstein.rom \
             at ~/.emu198x/roms/tatung-einstein/"
        );
    };
    let bios = fs::read(&path).expect("read BIOS");
    assert_eq!(bios.len(), 0x2000, "BIOS must be exactly 8 KB");

    let mut sys = Einstein::new(bios, EinsteinRegion::Pal);
    for _ in 0..300 {
        sys.run_frame();
    }

    // The MOS now boots all the way to its prompt — banner and `Ready`
    // text on the backdrop — instead of hanging at VDP init. That path
    // needed the $24 ROM-toggle, the WD1770 (so disk commands complete),
    // and a synthesised INDEX pulse. The booted screen is a dominant
    // backdrop colour with a few thousand pixels of text; the old hung
    // state was a single uniform colour.
    let fb = sys.framebuffer();
    assert!(!fb.is_empty(), "framebuffer should be allocated");
    let mut counts = std::collections::HashMap::new();
    for &px in fb {
        *counts.entry(px).or_insert(0usize) += 1;
    }
    assert!(
        counts.len() >= 2,
        "expected text over the backdrop; got {} colour(s) (bios: {})",
        counts.len(),
        path.display()
    );
    let backdrop = *counts.values().max().expect("non-empty framebuffer");
    let text = fb.len() - backdrop;
    assert!(
        text >= 500,
        "expected the MOS banner / prompt text; got {text} foreground pixels"
    );
}
