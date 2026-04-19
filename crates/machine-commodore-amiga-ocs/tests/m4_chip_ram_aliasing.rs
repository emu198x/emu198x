//! M4: chip-RAM aliasing via incomplete address decode.
//!
//! Per `wiki/decisions/amiga-restart-plan.md`. OCS Agnus on the
//! A500/A2000 has 19 chip-RAM address lines, so addresses above
//! `$7FFFF` wrap into the lower 512 KiB. The KS 1.3 boot exploits
//! this in Phase 7 (chip-RAM probe): writes a magic pattern to
//! progressively-higher addresses and detects the wrap point as
//! the top of installed RAM.
//!
//! With 512K installed, `$80000` should alias to `$0`,
//! `$100000` to `$0`, etc. — anywhere in the `$0-$1FFFFF` chip-RAM
//! decode range routes via `addr & 0x7FFFF`.

use std::path::PathBuf;
use machine_commodore_amiga_ocs::AmigaOcs;

fn load_kickstart() -> Option<Vec<u8>> {
    let home = std::env::var("HOME").expect("HOME is set");
    let path = PathBuf::from(home).join(".emu198x/roms/commodore-amiga/kick13.rom");
    if !path.exists() {
        eprintln!("skipping: Kickstart 1.3 ROM missing at {}", path.display());
        return None;
    }
    Some(std::fs::read(&path).expect("read Kickstart 1.3 ROM"))
}

#[test]
fn chip_ram_aliases_on_19_bit_decode() {
    let Some(rom) = load_kickstart() else { return };
    let mut amiga = AmigaOcs::new(rom);

    // Drive past OVL clear so chip RAM is visible at $0.
    for _ in 0..3_000_000 {
        amiga.tick_cck();
    }
    assert!(!amiga.memory().overlay());

    // Synthetic test: write magic patterns to addresses that should
    // wrap. Use poke_word so we go through the same path the CPU does.
    amiga.poke_word(0x000000, 0x1234); // baseline
    assert_eq!(amiga.read_word(0x000000), 0x1234);

    amiga.poke_word(0x080000, 0xCAFE); // should alias to $0
    assert_eq!(
        amiga.read_word(0x000000),
        0xCAFE,
        "$80000 should alias to $0 on 512K chip RAM"
    );
    assert_eq!(
        amiga.read_word(0x080000),
        0xCAFE,
        "Read at $80000 should also see the aliased byte"
    );

    amiga.poke_word(0x100000, 0xDEAD); // should alias to $0
    assert_eq!(amiga.read_word(0x000000), 0xDEAD);

    amiga.poke_word(0x180000, 0xBEEF); // should alias to $0
    assert_eq!(amiga.read_word(0x000000), 0xBEEF);

    // $200000 is OUTSIDE Gary's chip-RAM decode range (Unmapped on
    // OCS). Writes drop, reads return floating bus.
    amiga.poke_word(0x200000, 0x9999);
    assert_eq!(
        amiga.read_word(0x200000),
        0xFFFF,
        "$200000 is unmapped — reads as floating bus"
    );
    assert_ne!(
        amiga.read_word(0x000000),
        0x9999,
        "$200000 write should NOT alias into chip RAM"
    );
}
