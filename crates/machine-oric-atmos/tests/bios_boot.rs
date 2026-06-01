//! Oric Atmos BIOS boot smoke.

use std::env;
use std::fs;
use std::path::PathBuf;

use machine_oric_atmos::{OricAtmos, OricModel};

fn rom_path() -> Option<PathBuf> {
    if let Ok(p) = env::var("EMU198X_ORIC_ATMOS_ROM") {
        let p = PathBuf::from(p);
        if p.exists() {
            return Some(p);
        }
    }
    let home = env::var("HOME").ok()?;
    let dir = PathBuf::from(home).join(".emu198x/roms/oric-atmos");
    for name in ["atmos.rom", "oric1.rom"] {
        let p = dir.join(name);
        if p.exists() {
            return Some(p);
        }
    }
    None
}

#[test]
#[ignore = "needs Oric Atmos / Oric-1 ROM (16 KB) — run with --ignored"]
fn rom_boots_to_initial_screen() {
    let Some(path) = rom_path() else {
        panic!(
            "Oric ROM not found — set EMU198X_ORIC_ATMOS_ROM or place atmos.rom \
             / oric1.rom at ~/.emu198x/roms/oric-atmos/"
        );
    };
    let rom = fs::read(&path).expect("read ROM");
    assert_eq!(rom.len(), 0x4000, "ROM must be exactly 16 KB");

    let mut sys = OricAtmos::new(rom, OricModel::Atmos);
    for _ in 0..300 {
        sys.run_frame();
    }

    let fb = sys.framebuffer();
    assert_eq!(fb.len(), 240 * 224);
    let mut colours = std::collections::HashSet::new();
    for &px in fb {
        colours.insert(px);
        if colours.len() >= 8 {
            break;
        }
    }
    assert!(
        colours.len() >= 2,
        "framebuffer should have >= 2 distinct colours; got {} (rom: {})",
        colours.len(),
        path.display()
    );
    let non_zero = fb.iter().filter(|&&px| px & 0x00FF_FFFF != 0).count();
    assert!(
        non_zero >= 1024,
        "boot screen should have >= 1024 non-backdrop pixels; got {non_zero}"
    );
}
