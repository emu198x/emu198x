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

    // The boot screen is `Acorn Electron` / `BASIC` / `>` — white text on a
    // black background. This is a precise fingerprint: mostly black, a few
    // thousand white pixels of text, and crucially *no* red. A red field was
    // the signature of the scrambled palette decode, and a garbage screen-start
    // left raster noise rather than clean text.
    let count = |target: u32| fb.iter().filter(|&&px| px == target).count();
    let black = count(0xFF00_0000);
    let white = count(0xFFFF_FFFF);
    let red = count(0xFFFF_0000);

    assert!(
        black > fb.len() * 3 / 4,
        "background should be predominantly black; got {black} black pixels (os: {})",
        os_path.display()
    );
    assert!(
        (500..40_000).contains(&white),
        "expected the banner as white text; got {white} white pixels"
    );
    assert_eq!(red, 0, "no red pixels — the palette must decode correctly");
}
