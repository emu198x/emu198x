//! Memotech MTX ROM boot smoke.

use std::env;
use std::fs;
use std::path::PathBuf;

use machine_memotech_mtx::{Mtx, MtxModel};

fn rom_path() -> Option<PathBuf> {
    if let Ok(p) = env::var("EMU198X_MTX_ROM") {
        let p = PathBuf::from(p);
        if p.exists() {
            return Some(p);
        }
    }
    let home = env::var("HOME").ok()?;
    let p = PathBuf::from(home).join(".emu198x/roms/memotech-mtx/mtx.rom");
    p.exists().then_some(p)
}

#[test]
#[ignore = "needs a 16 KB Memotech MTX ROM — run with --ignored"]
fn rom_boots_without_panic() {
    let Some(path) = rom_path() else {
        panic!(
            "MTX ROM not found — set EMU198X_MTX_ROM or place mtx.rom (16 KB) \
             at ~/.emu198x/roms/memotech-mtx/"
        );
    };
    let rom = fs::read(&path).expect("read ROM");
    assert_eq!(rom.len(), 0x4000, "ROM must be exactly 16 KB");

    let mut sys = Mtx::new(rom, MtxModel::Mtx500).expect("init");
    for _ in 0..200 {
        sys.run_frame();
    }
    assert!(sys.frame_count() >= 200);
}
