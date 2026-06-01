//! ZX80 ROM boot smoke.

use std::env;
use std::fs;
use std::path::PathBuf;

use machine_sinclair_zx80::Zx80;

fn rom_path() -> Option<PathBuf> {
    if let Ok(p) = env::var("EMU198X_ZX80_ROM") {
        let p = PathBuf::from(p);
        if p.exists() {
            return Some(p);
        }
    }
    let home = env::var("HOME").ok()?;
    let p = PathBuf::from(home).join(".emu198x/roms/sinclair-zx80/zx80.rom");
    p.exists().then_some(p)
}

#[test]
#[ignore = "needs a 4 KB ZX80 ROM — run with --ignored"]
fn rom_boots_without_panic() {
    let Some(path) = rom_path() else {
        panic!(
            "ZX80 ROM not found — set EMU198X_ZX80_ROM or place zx80.rom \
             at ~/.emu198x/roms/sinclair-zx80/"
        );
    };
    let rom = fs::read(&path).expect("read ROM");
    assert_eq!(rom.len(), 0x1000, "ROM must be exactly 4 KB");

    let mut sys = Zx80::new(rom, 1024).expect("init");
    for _ in 0..200 {
        sys.run_frame();
    }
    assert!(sys.frame_count() >= 200);
}
