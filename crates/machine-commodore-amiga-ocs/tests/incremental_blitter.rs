//! End-to-end wiring of the incremental blitter (#31) through the
//! machine tick loop.
//!
//! The blitter no longer completes instantly on the BLTSIZE write — it
//! consumes two startup outcomes followed by at most one DMA operation
//! per granted CCK while the machine ticks. On this later original Agnus,
//! BBUSY (DMACONR bit 14) stays asserted until it finishes and INT_BLIT
//! fires on completion. These checks drive a real A→D copy through the
//! public bus (`poke_word`) and tick the machine to completion, the same
//! path a `WaitBlit` loop would exercise.

use machine_commodore_amiga_ocs::AmigaOcs;

const DMACONR: u32 = 0x00DF_F002;
const INTREQR: u32 = 0x00DF_F01E;
const BBUSY: u16 = 0x4000;
const INT_BLIT: u16 = 0x0040;

/// Build a machine with a dummy Kickstart — the test drives the blitter
/// directly and never runs ROM code.
fn machine() -> AmigaOcs {
    AmigaOcs::new(vec![0u8; 512 * 1024])
}

/// Program a 1-word A→D copy (minterm $F0) from `src` to `dst` and fire
/// BLTSIZE. Source/dest are chip-RAM word addresses.
fn program_copy(amiga: &mut AmigaOcs, src: u32, dst: u32) {
    // Enable master DMA + blitter DMA (DMACON: SETCLR | DMAEN | BLTEN).
    amiga.poke_word(0x00DF_F096, 0x8000 | 0x0200 | 0x0040);
    amiga.poke_word(0x00DF_F040, 0x0900 | 0x00F0); // BLTCON0: USEA|USED|minterm A
    amiga.poke_word(0x00DF_F042, 0x0000); // BLTCON1
    amiga.poke_word(0x00DF_F044, 0xFFFF); // BLTAFWM
    amiga.poke_word(0x00DF_F046, 0xFFFF); // BLTALWM
    amiga.poke_word(0x00DF_F050, (src >> 16) as u16); // BLTAPTH
    amiga.poke_word(0x00DF_F052, (src & 0xFFFF) as u16); // BLTAPTL
    amiga.poke_word(0x00DF_F054, (dst >> 16) as u16); // BLTDPTH
    amiga.poke_word(0x00DF_F056, (dst & 0xFFFF) as u16); // BLTDPTL
    amiga.poke_word(0x00DF_F058, (1 << 6) | 1); // BLTSIZE: 1 row × 1 word
}

fn chip_word(amiga: &AmigaOcs, addr: u32) -> u16 {
    let hi = u16::from(amiga.read_chip_ram_byte(addr));
    let lo = u16::from(amiga.read_chip_ram_byte(addr + 1));
    (hi << 8) | lo
}

#[test]
fn blit_drains_over_ccks_sets_bbusy_then_completes() {
    let mut amiga = machine();
    let src = 0x0002_0000;
    let dst = 0x0003_0000;
    amiga.poke_word(src, 0xABCD); // chip RAM source

    program_copy(&mut amiga, src, dst);

    // The blit is now in flight — BBUSY asserted, dest not yet written.
    assert_ne!(
        amiga.read_word(DMACONR) & BBUSY,
        0,
        "BBUSY must be set immediately after BLTSIZE (blit is in flight)"
    );

    // Tick until BBUSY clears — the WaitBlit path.
    let mut ticks = 0u32;
    while amiga.read_word(DMACONR) & BBUSY != 0 {
        amiga.tick();
        ticks += 1;
        assert!(ticks < 100_000, "blit never completed (BBUSY stuck)");
    }

    assert!(
        ticks > 0,
        "blit must take real chip cycles, not finish instantly"
    );
    assert_eq!(
        chip_word(&amiga, dst),
        0xABCD,
        "A→D copy must land the source word in the destination"
    );
    assert_ne!(
        amiga.read_word(INTREQR) & INT_BLIT,
        0,
        "INT_BLIT must be raised when the blit completes"
    );
}

#[test]
fn bbusy_clear_when_idle() {
    // No blit started → BBUSY must read clear.
    let amiga = machine();
    assert_eq!(
        amiga.read_word(DMACONR) & BBUSY,
        0,
        "BBUSY must be clear with no blit in flight"
    );
}
