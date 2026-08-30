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
    let p = PathBuf::from(home).join(".emu198x/roms/jupiter-ace/ace.rom");
    p.exists().then_some(p)
}

#[test]
#[ignore = "FIXTURE: needs an 8 KB Jupiter Ace Forth ROM — run with --ignored"]
fn rom_boots_to_cursor() {
    let Some(path) = rom_path() else {
        panic!(
            "Jupiter Ace ROM not found — set EMU198X_JUPITER_ACE_ROM or place \
             ace.rom at ~/.emu198x/roms/jupiter-ace/"
        );
    };
    let rom = fs::read(&path).expect("read ROM");
    assert_eq!(rom.len(), 0x2000, "ROM must be exactly 8 KB");

    let mut sys = JupiterAce::new(rom, 0).expect("init");
    for _ in 0..200 {
        sys.run_frame();
    }
    assert!(sys.frame_count() >= 200);

    // The cold start must copy the character set into character RAM ($2800)
    // — proving the boot actually ran, not merely that it didn't panic. A
    // blank char RAM was the signature of the old swapped video/char map,
    // which left the screen full of vertical-line noise.
    let font_bytes = (0x2800u16..0x2C00).filter(|&a| sys.peek(a) != 0).count();
    assert!(
        font_bytes > 400,
        "character RAM should hold the copied font; got {font_bytes} non-zero bytes"
    );

    // The boot screen is blank with a single cursor block at the bottom
    // left: the framebuffer is overwhelmingly one paper colour, with a
    // small second colour for the cursor.
    let fb = sys.framebuffer();
    assert_eq!(
        fb.len(),
        (sys.framebuffer_width() * sys.framebuffer_height()) as usize
    );
    let mut counts = std::collections::HashMap::new();
    for &px in fb {
        *counts.entry(px).or_insert(0usize) += 1;
    }
    let paper = *counts.values().max().expect("non-empty framebuffer");
    assert!(
        paper > fb.len() * 9 / 10,
        "boot screen should be mostly one paper colour; got {paper}/{}",
        fb.len()
    );
    let ink = fb.len() - paper;
    assert!(
        (1..5000).contains(&ink),
        "expected a small cursor block; got {ink} non-paper pixels"
    );
}
