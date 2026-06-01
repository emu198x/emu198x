//! Jupiter Ace ROM boot smoke.

use std::env;
use std::fs;
use std::path::PathBuf;

use machine_jupiter_ace::JupiterAce;

fn rom_path() -> Option<PathBuf> {
    if let Ok(p) = env::var("EMU198X_JUPITER_ACE_ROM") {
        let p = PathBuf::from(p);
        if p.exists() {
            return Some(p);
        }
    }
    let home = env::var("HOME").ok()?;
    let p = PathBuf::from(home).join(".emu198x/roms/jupiter-ace/jupiter-ace.rom");
    p.exists().then_some(p)
}

#[test]
#[ignore = "needs an 8 KB Jupiter Ace Forth ROM — run with --ignored"]
fn rom_boots_without_panic() {
    let Some(path) = rom_path() else {
        panic!(
            "Jupiter Ace ROM not found — set EMU198X_JUPITER_ACE_ROM or place \
             jupiter-ace.rom at ~/.emu198x/roms/jupiter-ace/"
        );
    };
    let rom = fs::read(&path).expect("read ROM");
    assert_eq!(rom.len(), 0x2000, "ROM must be exactly 8 KB");

    let mut sys = JupiterAce::new(rom, 3 * 1024).expect("init");
    for _ in 0..200 {
        sys.run_frame();
    }
    assert!(sys.frame_count() >= 200);
    assert_eq!(
        sys.framebuffer().len(),
        (sys.framebuffer_width() * sys.framebuffer_height()) as usize
    );
}
