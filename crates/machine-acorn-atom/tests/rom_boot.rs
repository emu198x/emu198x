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
fn rom_boots_to_prompt() {
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

    // The MOS cold start clears the screen and prints `ACORN ATOM` with a
    // `>` prompt. Without the CPU reset the screen stayed on the uninitialised
    // character grid (every cell the same code). Count distinct codes in the
    // text RAM at $8000: a real boot has the banner letters plus the cleared
    // background — several distinct codes, not one.
    use std::collections::HashSet;
    let codes: HashSet<u8> = (0x8000u16..0x8200).map(|a| sys.peek(a)).collect();
    assert!(
        codes.len() >= 4,
        "expected banner text in screen RAM (>= 4 distinct codes); got {} (rom: {})",
        codes.len(),
        path.display()
    );

    // And the framebuffer is the right size, mostly background with a little
    // foreground text.
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
        paper < fb.len(),
        "boot screen should not be a single flat colour"
    );
}
