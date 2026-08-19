//! M2: custom-register storage.
//!
//! Per `knowledge/decisions/amiga-restart-plan.md`. The KS 1.3 boot writes
//! to several custom registers in its first ~30 instructions
//! (verified from disassembly at $FC00FE-$FC0136):
//!
//! ```
//! $FC0118: MOVE.W $7FFF, $9A(A4)   ; INTENA  = $7FFF (clear all)
//! $FC011C: MOVE.W $7FFF, $9C(A4)   ; INTREQ  = $7FFF (clear all)
//! $FC0120: MOVE.W $7FFF, $96(A4)   ; DMACON  = $7FFF (clear all)
//! $FC0124: MOVE.W #$0200, $100(A4) ; BPLCON0 = $0200 (no bitplanes,
//!                                              COLOR enable)
//! $FC0130: MOVE.W #$0444, $180(A4) ; COLOR00 = $0444 (mid grey)
//! ```
//!
//! M2 stores these registers as plain variables; no behaviour wired
//! yet (no DMA, no copper, no display). Set/clear semantics for the
//! INTENA/INTREQ/DMACON triple: bit 15 = set if 1, clear if 0.

use machine_commodore_amiga_ocs::AmigaOcs;
use std::path::PathBuf;

fn load_kickstart() -> Option<Vec<u8>> {
    let home = std::env::var("HOME").expect("HOME is set");
    let path = PathBuf::from(home).join(".emu198x/roms/commodore-amiga/kick13.rom");
    if !path.exists() {
        emu198x_test_skip::record(&format!(
            "skipping: Kickstart 1.3 ROM missing at {}",
            path.display()
        ));
        return None;
    }
    Some(std::fs::read(&path).expect("read Kickstart 1.3 ROM"))
}

#[test]
fn boot_clears_intena_intreq_dmacon_then_sets_bplcon0_color00() {
    let Some(rom) = load_kickstart() else { return };
    let mut amiga = AmigaOcs::new(rom);

    // Tick well past the early init sequence. The boot has a
    // ~131K-iteration busy-wait delay loop at $FC00DE before the
    // diag-ROM probe — ~1.5M CCKs at ~12 CCK/iter. 2M CCKs (~280ms
    // emulated) covers the delay plus the early register writes.
    for _ in 0..2_000_000 {
        amiga.tick();
    }

    // After the clear-all writes ($7FFF with bit 15 = 0 = clear),
    // these registers should be zero — except INTREQ, which may
    // have latched VERTB by the time we sample (M6 fires VBL
    // every PAL frame regardless of whether the boot has installed
    // a handler). Mask VERTB out of the INTREQ check.
    assert_eq!(
        amiga.intena(),
        0,
        "INTENA should be cleared by boot's MOVE.W #$7FFF"
    );
    assert_eq!(
        amiga.intreq() & !0x0020,
        0,
        "INTREQ (excluding VERTB latched by Agnus) should be cleared"
    );
    assert_eq!(amiga.dmacon(), 0, "DMACON should be cleared");
    // BPLCON0 = $0200 — set once and not changed until much later in
    // boot. After 2M CCKs we should still see this value (BPU=0 +
    // COLOR enable).
    assert_eq!(
        amiga.bplcon0(),
        0x0200,
        "BPLCON0 should be set to $0200 by boot"
    );
    // COLOR00 — boot writes $0444 (mid grey) initially as a
    // "booting in progress" indicator, then writes other colour
    // values as boot progresses through phases. Don't pin to a
    // specific value; just verify SOMETHING was written (non-zero
    // and within the 12-bit RGB range).
    let color00 = amiga.color(0);
    assert_ne!(
        color00, 0,
        "COLOR00 should be non-zero (boot writes status colours)"
    );
    assert!(
        color00 <= 0x0FFF,
        "COLOR00 ${color00:04X} should be a valid 12-bit RGB value"
    );
}

#[test]
fn intena_set_clear_semantics() {
    // Synthetic test of the set/clear semantics independent of the
    // real boot: bit 15 = 1 means "set the bits in 14..0"; bit 15 = 0
    // means "clear them".
    let Some(rom) = load_kickstart() else { return };
    let mut amiga = AmigaOcs::new(rom);

    // Drive directly via the bus by writing to $DFF09A.
    amiga.poke_word(0x00DFF09A, 0x8042); // SET bits 6 (BLIT) + 1
    assert_eq!(amiga.intena(), 0x0042);

    amiga.poke_word(0x00DFF09A, 0x8004); // ALSO set bit 2 (SOFTINT)
    assert_eq!(amiga.intena(), 0x0046);

    amiga.poke_word(0x00DFF09A, 0x0040); // CLEAR bit 6
    assert_eq!(amiga.intena(), 0x0006);
}
