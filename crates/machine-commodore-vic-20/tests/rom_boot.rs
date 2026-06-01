//! Commodore VIC-20 ROM boot smoke.

use std::env;
use std::fs;
use std::path::PathBuf;

use machine_commodore_vic_20::{Vic20, Vic20Model};

fn rom(env: &str, default_name: &str) -> Option<Vec<u8>> {
    if let Ok(p) = env::var(env) {
        let p = PathBuf::from(p);
        if p.exists() {
            return fs::read(&p).ok();
        }
    }
    let home = env::var("HOME").ok()?;
    let p = PathBuf::from(home).join(format!(".emu198x/roms/commodore-vic-20/{default_name}"));
    p.exists().then(|| fs::read(&p).ok()).flatten()
}

#[test]
#[ignore = "needs VIC-20 ROM set — run with --ignored"]
fn rom_set_boots_without_panic() {
    let kernal = rom("EMU198X_VIC20_KERNAL", "kernal.rom");
    let basic = rom("EMU198X_VIC20_BASIC", "basic.rom");
    let char_rom = rom("EMU198X_VIC20_CHAR", "char.rom");

    let (Some(kernal), Some(basic), Some(char_rom)) = (kernal, basic, char_rom) else {
        panic!(
            "VIC-20 ROM set incomplete — place kernal.rom (8 KB) / basic.rom (8 KB) / \
             char.rom (4 KB) under ~/.emu198x/roms/commodore-vic-20/"
        );
    };

    let mut sys = Vic20::new(kernal, basic, char_rom, Vic20Model::Pal, 0);
    for _ in 0..200 {
        sys.run_frame();
    }
    assert!(sys.frame_count() >= 200);
}
