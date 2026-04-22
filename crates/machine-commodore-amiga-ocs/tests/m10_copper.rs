//! M10: Agnus Copper.
//!
//! Per `wiki/decisions/amiga-restart-plan.md`. The Copper is the
//! display-list coprocessor inside Agnus. It reads instruction
//! pairs (32-bit each) from chip RAM and writes to chipset registers
//! at specific beam positions, building per-frame display setups.
//!
//! Three instructions:
//!   MOVE  reg, val  (word1 = reg<<1 with bit0=0; word2 = val)
//!   WAIT  vp, hp    (word1 = vp<<8 | hp with bit0=1; word2 = mask | 0)
//!   SKIP  vp, hp    (same shape; word2 bit0 = 1)
//!
//! End of list = `WAIT $FF, $FE` ($FFFFFFFE) — waits for an
//! impossible beam position.
//!
//! M10 minimum: run the copper, execute MOVE / WAIT / SKIP correctly,
//! gated by DMACON.COPEN (bit 7) and DMAEN (master, bit 9). No DMA
//! slot scheduling yet — copper just runs at one instruction per
//! 4 CCKs when enabled.

use machine_commodore_amiga_ocs::AmigaOcs;
use std::path::PathBuf;

fn load_kickstart() -> Option<Vec<u8>> {
    let home = std::env::var("HOME").expect("HOME is set");
    let path = PathBuf::from(home).join(".emu198x/roms/commodore-amiga/kick13.rom");
    if !path.exists() {
        eprintln!("skipping: Kickstart 1.3 ROM missing at {}", path.display());
        return None;
    }
    Some(std::fs::read(&path).expect("read Kickstart 1.3 ROM"))
}

/// Helper: place a copper list at the given chip-RAM address.
/// Each tuple is (word1, word2). End-of-list `(0xFFFF, 0xFFFE)`
/// must terminate the list.
fn write_copper_list(amiga: &mut AmigaOcs, addr: u32, list: &[(u16, u16)]) {
    for (i, (w1, w2)) in list.iter().enumerate() {
        let off = addr + (i as u32) * 4;
        amiga.poke_word(off, *w1);
        amiga.poke_word(off + 2, *w2);
    }
}

#[test]
fn copper_executes_simple_move_list() {
    let Some(rom) = load_kickstart() else { return };
    let mut amiga = AmigaOcs::new(rom);

    // Drop overlay so chip RAM is visible at $0 (we write there).
    // Easiest: poke CIA-A DDRA + PRA to clear OVL.
    amiga.poke_byte(0x00BFE201, 0x03);
    amiga.poke_byte(0x00BFE001, 0x02);
    assert!(!amiga.memory().overlay());

    // Build a copper list at $1000:
    //   MOVE COLOR00 ($180) = $0F00 (red)
    //   MOVE COLOR01 ($182) = $00F0 (green)
    //   WAIT $FF, $FE — end-of-list
    write_copper_list(
        &mut amiga,
        0x1000,
        &[
            (0x0180, 0x0F00), // MOVE COLOR00 = $0F00
            (0x0182, 0x00F0), // MOVE COLOR01 = $00F0
            (0xFFFF, 0xFFFE), // end-of-list
        ],
    );

    // Set COP1LC = $1000 (high half then low).
    amiga.poke_word(0x00DFF080, 0x0000); // COP1LCH
    amiga.poke_word(0x00DFF082, 0x1000); // COP1LCL

    // COPJMP1 strobe — triggers copper to start at COP1LC.
    amiga.poke_word(0x00DFF088, 0x0000);

    // DMACON: enable DMAEN (bit 9) + COPEN (bit 7) via set/clear.
    amiga.poke_word(0x00DFF096, 0x8280);

    // Tick enough CCKs for copper to execute MOVE + MOVE + reach WAIT.
    // Each instruction = 4 CCKs (2 words × 2 CCKs/word).
    for _ in 0..32 {
        amiga.tick();
    }

    assert_eq!(
        amiga.color(0) & 0x0FFF,
        0x0F00,
        "Copper MOVE should write COLOR00 = $0F00"
    );
    assert_eq!(
        amiga.color(1) & 0x0FFF,
        0x00F0,
        "Copper MOVE should write COLOR01 = $00F0"
    );
}

#[test]
fn copper_does_not_run_without_copen() {
    let Some(rom) = load_kickstart() else { return };
    let mut amiga = AmigaOcs::new(rom);
    amiga.poke_byte(0x00BFE201, 0x03);
    amiga.poke_byte(0x00BFE001, 0x02);

    write_copper_list(&mut amiga, 0x1000, &[(0x0180, 0x0FFF), (0xFFFF, 0xFFFE)]);
    amiga.poke_word(0x00DFF080, 0x0000);
    amiga.poke_word(0x00DFF082, 0x1000);
    amiga.poke_word(0x00DFF088, 0x0000); // COPJMP1

    // Note: NO DMACON write — copper should remain inactive.
    for _ in 0..32 {
        amiga.tick();
    }
    assert_eq!(
        amiga.color(0),
        0,
        "Copper must not execute when DMACON.COPEN is off"
    );
}

#[test]
fn copper_wait_pauses_until_beam_reaches_target() {
    let Some(rom) = load_kickstart() else { return };
    let mut amiga = AmigaOcs::new(rom);
    amiga.poke_byte(0x00BFE201, 0x03);
    amiga.poke_byte(0x00BFE001, 0x02);

    // List:
    //   MOVE COLOR00 = $0F00
    //   WAIT vp=$05 hp=$00 (wait until line 5)
    //   MOVE COLOR01 = $00F0
    //   END
    write_copper_list(
        &mut amiga,
        0x1000,
        &[
            (0x0180, 0x0F00), // MOVE
            (0x0501, 0xFFFE), // WAIT v=5, h=0
            (0x0182, 0x00F0), // MOVE
            (0xFFFF, 0xFFFE), // END
        ],
    );
    amiga.poke_word(0x00DFF080, 0x0000);
    amiga.poke_word(0x00DFF082, 0x1000);
    amiga.poke_word(0x00DFF088, 0x0000);
    amiga.poke_word(0x00DFF096, 0x8280);

    // After a few CCKs, COLOR00 should be set but COLOR01 should NOT
    // (we haven't reached line 5 yet).
    for _ in 0..16 {
        amiga.tick();
    }
    assert_eq!(
        amiga.color(0) & 0x0FFF,
        0x0F00,
        "first MOVE should have run"
    );
    assert_eq!(amiga.color(1) & 0x0FFF, 0x0000, "post-WAIT MOVE blocked");

    // Tick to line 5 (5 lines × 454 ticks/line = 2270 ticks).
    for _ in 0..3000 {
        amiga.tick();
    }
    assert_eq!(
        amiga.color(1) & 0x0FFF,
        0x00F0,
        "post-WAIT MOVE should have run after beam reached line 5"
    );
}
