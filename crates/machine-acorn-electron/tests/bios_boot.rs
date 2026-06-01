//! Acorn Electron BIOS boot smoke.
//!
//! Loads the 16 KB OS ROM + 16 KB BASIC ROM from the user's local
//! ROM directory and verifies the boot screen renders non-trivial
//! framebuffer within 200 frames. Gated `#[ignore]` because both
//! ROMs are copyrighted and not shipped in-tree.
//!
//! ROM source (first match wins):
//!   1. `EMU198X_ELECTRON_OS` + `EMU198X_ELECTRON_BASIC` env vars
//!   2. `~/.emu198x/roms/acorn-electron/os.rom` +
//!      `~/.emu198x/roms/acorn-electron/basic.rom`

use std::env;
use std::fs;
use std::path::PathBuf;

use machine_acorn_electron::AcornElectron;

fn rom_path(env_key: &str, default_name: &str) -> Option<PathBuf> {
    if let Ok(p) = env::var(env_key) {
        let p = PathBuf::from(p);
        if p.exists() {
            return Some(p);
        }
    }
    let home = env::var("HOME").ok()?;
    let p = PathBuf::from(home).join(format!(".emu198x/roms/acorn-electron/{default_name}"));
    p.exists().then_some(p)
}

#[test]
#[ignore = "needs Acorn Electron OS ROM + BASIC ROM (16 KB each) — run with --ignored"]
fn os_basic_roms_boot_to_initial_screen() {
    let Some(os_path) = rom_path("EMU198X_ELECTRON_OS", "os.rom") else {
        panic!(
            "Electron OS ROM not found — set EMU198X_ELECTRON_OS or place os.rom \
             at ~/.emu198x/roms/acorn-electron/"
        );
    };
    let Some(basic_path) = rom_path("EMU198X_ELECTRON_BASIC", "basic.rom") else {
        panic!(
            "Electron BASIC ROM not found — set EMU198X_ELECTRON_BASIC or place \
             basic.rom at ~/.emu198x/roms/acorn-electron/"
        );
    };
    let os = fs::read(&os_path).expect("read OS");
    let basic = fs::read(&basic_path).expect("read BASIC");
    assert_eq!(os.len(), 0x4000, "OS ROM must be 16 KB");
    assert_eq!(basic.len(), 0x4000, "BASIC ROM must be 16 KB");

    let mut sys = AcornElectron::new(os, basic);
    for _ in 0..200 {
        sys.run_frame();
    }

    let fb = sys.framebuffer();
    assert_eq!(fb.len(), 640 * 256);
    let mut colours = std::collections::HashSet::new();
    for &px in fb {
        colours.insert(px);
        if colours.len() >= 8 {
            break;
        }
    }
    assert!(
        colours.len() >= 2,
        "framebuffer should have >= 2 distinct colours; got {} (os: {})",
        colours.len(),
        os_path.display()
    );
    let non_zero = fb.iter().filter(|&&px| px & 0x00FF_FFFF != 0).count();
    assert!(
        non_zero >= 1024,
        "boot screen should have >= 1024 non-backdrop pixels; got {non_zero}"
    );
}
