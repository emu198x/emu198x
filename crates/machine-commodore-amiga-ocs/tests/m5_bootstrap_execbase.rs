//! M5: bootstrap ExecBase placement.
//!
//! Per `wiki/decisions/amiga-restart-plan.md`. The KS 1.3 boot
//! allocates the bootstrap ExecBase in chip RAM during what V37
//! calls Phase 8 ($F801FE-$F8022A). The bootstrap allocator (real
//! V34's equivalent) places a $57C-byte block; ExecBase pointer =
//! block + $318 (the size of the negative jump-table portion).
//!
//! In chip-only A500 KS 1.3 (per the archived investigation in
//! `wiki/decisions/amiga-chip-only-boot-failure.md`), this lands at
//! `$00000676`. ChkBase at ExecBase+$26 holds the one's-complement
//! of ExecBase as a sanity check.
//!
//! No new emulator behaviour — purely a regression check that
//! M0-M4 carry the boot far enough to reach Phase 8.

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
fn boot_places_execbase_in_chip_ram() {
    let Some(rom) = load_kickstart() else { return };
    let mut amiga = AmigaOcs::new(rom);

    // Phase 8 (ExecBase construction) follows Phase 7 (chip RAM
    // probe), which itself follows the busy-wait delay. ~5M CCKs
    // (~700ms emulated) is comfortably past Phase 8.
    for _ in 0..5_000_000 {
        amiga.tick();
    }

    // ExecBase pointer at $00000004 should hold a chip-RAM address.
    let exec_base = amiga.read_long(0x000004);
    assert!(
        (0x0000_0400..0x0008_0000).contains(&exec_base),
        "ExecBase ${exec_base:08X} should live in chip RAM ($400-$80000)"
    );
    assert_eq!(
        exec_base & 1,
        0,
        "ExecBase ${exec_base:08X} should be word-aligned"
    );

    // ChkBase at ExecBase+$26 = one's-complement of ExecBase.
    let chk_base = amiga.read_long(exec_base.wrapping_add(0x26));
    assert_eq!(
        chk_base, !exec_base,
        "ChkBase ${chk_base:08X} should be ~ExecBase ${exec_base:08X}"
    );

    // From the archived investigation we know the chip-only KS 1.3
    // path lands ExecBase at $00000676 specifically.
    assert_eq!(
        exec_base, 0x0000_0676,
        "Chip-only KS 1.3 should place ExecBase at $0676 \
         (matches archived investigation)"
    );
}
