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
#[ignore = "FIXTURE: needs VIC-20 ROM set — run with --ignored"]
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

    // The boot must reach the BASIC banner — not just run without panicking.
    // Two bugs used to stop it: the CPU was never reset (so it powered on at
    // PC=$0000, ran the BRK there, and stormed in the IRQ handler — black
    // screen), and the memory map mirrored the C64's (BASIC at $A000, KERNAL
    // mirrored at $C000) so `JMP ($C000)` read the wrong cold-start vector and
    // derailed. With both fixed the VIC-20 reaches its `**** CBM BASIC V2 ****`
    // screen.

    // Screen colour register: cyan border (3) + white background (1) = $1B,
    // set by CINT. Black ($00) means the boot never reached screen init.
    assert_eq!(
        sys.peek(0x900F),
        0x1B,
        "VIC screen-colour register should be cyan/white after boot"
    );

    // The banner text lands in screen RAM at $1E00 (unexpanded). Count cells
    // that are neither space ($20) nor null — a cleared-but-stuck screen has
    // none; the booted banner has dozens.
    let printed = (0x1E00u16..0x1EE0)
        .filter(|&a| !matches!(sys.peek(a), 0x20 | 0x00))
        .count();
    assert!(
        printed >= 20,
        "expected the BASIC banner in screen RAM; got {printed} printed cells"
    );
}
