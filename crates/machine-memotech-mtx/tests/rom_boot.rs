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
#[ignore = "FIXTURE: needs a Memotech MTX ROM set — run with --ignored"]
fn rom_boots_without_panic() {
    let Some(path) = rom_path() else {
        panic!(
            "MTX ROM not found — set EMU198X_MTX_ROM or place mtx.rom \
             at ~/.emu198x/roms/memotech-mtx/"
        );
    };
    let rom = fs::read(&path).expect("read ROM");
    // The 8 KB OS ROM plus at least one 8 KB paged ROM — `Mtx::new`'s own
    // contract, rather than a fixed size. This asserted exactly 16 KB
    // (OS + BASIC) and so rejected the 24 KB OS + BASIC + Assembler set the
    // machine is normally run with, failing before it reached the boot it
    // exists to smoke-test.
    assert!(
        rom.len() >= 0x4000 && rom.len().is_multiple_of(0x2000),
        "ROM must be the 8 KB OS plus at least one 8 KB paged ROM, got {} bytes",
        rom.len()
    );

    let mut sys = Mtx::new(rom, MtxModel::Mtx500).expect("init");
    for _ in 0..200 {
        sys.run_frame();
    }
    assert!(sys.frame_count() >= 200);
}
