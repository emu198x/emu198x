//! Commodore PET ROM boot smoke.

use std::env;
use std::fs;
use std::path::PathBuf;

use machine_commodore_pet::Pet;

fn rom(name: &str, env: &str, default_name: &str) -> Option<Vec<u8>> {
    if let Ok(p) = env::var(env) {
        let p = PathBuf::from(p);
        if p.exists() {
            return fs::read(&p).ok();
        }
    }
    let home = env::var("HOME").ok()?;
    let p = PathBuf::from(home).join(format!(".emu198x/roms/commodore-pet/{default_name}"));
    let _ = name;
    p.exists().then(|| fs::read(&p).ok()).flatten()
}

#[test]
#[ignore = "needs Commodore PET ROM set — run with --ignored"]
fn rom_set_boots_without_panic() {
    let kernal = rom("kernal", "EMU198X_PET_KERNAL", "kernal.rom");
    let basic = rom("basic", "EMU198X_PET_BASIC", "basic.rom");
    let editor = rom("editor", "EMU198X_PET_EDITOR", "editor.rom");
    let char_rom = rom("char", "EMU198X_PET_CHAR", "char.rom");

    let (Some(kernal), Some(basic), Some(editor), Some(char_rom)) =
        (kernal, basic, editor, char_rom)
    else {
        panic!(
            "PET ROM set incomplete — place kernal.rom (4 KB) / basic.rom (8 KB) / \
             editor.rom (2 KB) / char.rom (4 KB) under ~/.emu198x/roms/commodore-pet/"
        );
    };

    let mut sys = Pet::new(kernal, basic, editor, char_rom, 40);
    for _ in 0..200 {
        sys.run_frame();
    }
    assert!(sys.frame_count() >= 200);
    assert_eq!(
        sys.framebuffer().len(),
        (sys.framebuffer_width() * sys.framebuffer_height()) as usize
    );
}
