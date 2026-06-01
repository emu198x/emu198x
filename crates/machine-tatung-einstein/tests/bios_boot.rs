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
#[ignore = "needs Tatung Einstein X-TAL MOS ROM (8 KB) — run with --ignored"]
fn bios_boots_to_initial_screen() {
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

    let fb = sys.framebuffer();
    assert_eq!(fb.len(), 256 * 192);
    let mut colours = std::collections::HashSet::new();
    for &px in fb {
        colours.insert(px);
        if colours.len() >= 16 {
            break;
        }
    }
    // Honest check for the current state: the X-TAL MOS BIOS sets a
    // non-black VDP backdrop colour (typically blue) then hangs
    // waiting for the WD1770 floppy controller, which is not modelled
    // in this initial port. Text output never appears — the
    // framebuffer is uniformly the backdrop colour. Asserting
    // `non_zero >= 1024` confirms the VDP-init stage was reached
    // (display enabled, backdrop set to a non-black colour).
    let non_zero = fb.iter().filter(|&&px| px & 0x00FF_FFFF != 0).count();
    assert!(
        non_zero >= 1024,
        "BIOS should have reached VDP-init (>= 1024 non-black pixels); \
         got {non_zero} (bios: {}, colours: {})",
        path.display(),
        colours.len()
    );
}
