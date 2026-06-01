//! Acorn Atom ROM boot smoke.

use std::env;
use std::fs;
use std::path::PathBuf;

use machine_acorn_atom::AcornAtom;

fn rom_path() -> Option<PathBuf> {
    if let Ok(p) = env::var("EMU198X_ATOM_ROM") {
        let p = PathBuf::from(p);
        if p.exists() {
            return Some(p);
        }
    }
    let home = env::var("HOME").ok()?;
    let p = PathBuf::from(home).join(".emu198x/roms/acorn-atom/atom.rom");
    p.exists().then_some(p)
}

#[test]
#[ignore = "needs a 24 KB Acorn Atom combined ROM — run with --ignored"]
fn rom_boots_without_panic() {
    let Some(path) = rom_path() else {
        panic!(
            "Atom ROM not found — set EMU198X_ATOM_ROM or place atom.rom (24 KB) \
             at ~/.emu198x/roms/acorn-atom/"
        );
    };
    let rom = fs::read(&path).expect("read ROM");
    assert_eq!(rom.len(), 0x6000, "ROM must be exactly 24 KB");

    let mut sys = AcornAtom::new(rom, 0x0A00);
    for _ in 0..200 {
        sys.run_frame();
    }
    assert!(sys.frame_count() >= 200);
}
