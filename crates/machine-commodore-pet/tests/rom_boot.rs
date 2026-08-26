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
#[ignore = "FIXTURE: needs Commodore PET ROM set — run with --ignored"]
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

    // The boot must reach the BASIC banner, not merely run without panicking.
    // Three bugs used to stop it short: the CPU was never reset (so it powered
    // on at PC=$0000 and stuck on the uninitialised "@" grid); the character
    // ROM was addressed with a 16-byte stride instead of 8 (every glyph read
    // its neighbour and "spaces" rendered as line noise); and the CRTC
    // pre-incremented its address counter, dropping the first cell of each row.
    //
    // Screen RAM sits at $8000. The `### COMMODORE BASIC ###` / `BYTES FREE` /
    // `READY.` banner fills the first few rows with non-space screen codes;
    // a screen stuck before init is all spaces ($20) or nulls.
    let printed = (0x8000u16..0x8078)
        .filter(|&a| !matches!(sys.peek(a), 0x20 | 0x00))
        .count();
    assert!(
        printed >= 20,
        "expected the BASIC banner in screen RAM; got {printed} printed cells"
    );
}
