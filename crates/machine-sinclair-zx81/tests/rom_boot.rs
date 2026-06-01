//! ZX81 ROM boot smoke.

use std::env;
use std::fs;
use std::path::PathBuf;

use machine_sinclair_zx81::Zx81;

fn rom_path() -> Option<PathBuf> {
    if let Ok(p) = env::var("EMU198X_ZX81_ROM") {
        let p = PathBuf::from(p);
        if p.exists() {
            return Some(p);
        }
    }
    let home = env::var("HOME").ok()?;
    let p = PathBuf::from(home).join(".emu198x/roms/sinclair-zx81/zx81.rom");
    p.exists().then_some(p)
}

#[test]
#[ignore = "needs an 8 KB ZX81 ROM — run with --ignored"]
fn rom_boots_without_panic() {
    let Some(path) = rom_path() else {
        panic!(
            "ZX81 ROM not found — set EMU198X_ZX81_ROM or place zx81.rom \
             at ~/.emu198x/roms/sinclair-zx81/"
        );
    };
    let rom = fs::read(&path).expect("read ROM");
    assert_eq!(rom.len(), 0x2000, "ROM must be exactly 8 KB");

    let mut sys = Zx81::new(rom, 16384).expect("init");
    for _ in 0..200 {
        sys.run_frame();
    }
    assert!(sys.frame_count() >= 200);
}
