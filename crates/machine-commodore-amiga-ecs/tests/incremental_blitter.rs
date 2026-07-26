//! End-to-end wiring of the incremental blitter (#31) through the ECS
//! machine tick loop, including the ECS large-blit start path
//! (BLTSIZV/BLTSIZH at $05C/$05E) that KS 2.x/3.x uses for most blits.
//!
//! The blit consumes two startup outcomes followed by at most one DMA
//! operation per granted CCK. BBUSY stays asserted in DMACONR until it
//! finishes, and INT_BLIT fires on completion.

use machine_commodore_amiga_ecs::AmigaEcs;

const DMACONR: u32 = 0x00DF_F002;
const INTREQR: u32 = 0x00DF_F01E;
const BBUSY: u16 = 0x4000;
const INT_BLIT: u16 = 0x0040;

fn machine() -> AmigaEcs {
    AmigaEcs::new(vec![0u8; 512 * 1024])
}

fn chip_word(amiga: &AmigaEcs, addr: u32) -> u16 {
    let hi = u16::from(amiga.read_chip_ram_byte(addr));
    let lo = u16::from(amiga.read_chip_ram_byte(addr + 1));
    (hi << 8) | lo
}

/// Tick to BBUSY-clear (the WaitBlit path), returning the tick count.
fn run_to_idle(amiga: &mut AmigaEcs) -> u32 {
    let mut ticks = 0u32;
    while amiga.read_word(DMACONR) & BBUSY != 0 {
        amiga.tick();
        ticks += 1;
        assert!(ticks < 100_000, "blit never completed (BBUSY stuck)");
    }
    ticks
}

#[test]
fn ecs_large_blit_start_path_drains_over_ccks() {
    // Start via the ECS BLTSIZV/BLTSIZH path ($05C/$05E), a 1-word A→D
    // copy (minterm $F0).
    let mut amiga = machine();
    let src = 0x0002_0000;
    let dst = 0x0003_0000;
    amiga.poke_word(src, 0xBEEF);

    amiga.poke_word(0x00DF_F096, 0x8000 | 0x0200 | 0x0040); // DMAEN | BLTEN
    amiga.poke_word(0x00DF_F040, 0x0900 | 0x00F0); // BLTCON0: USEA|USED|minterm A
    amiga.poke_word(0x00DF_F042, 0x0000); // BLTCON1
    amiga.poke_word(0x00DF_F044, 0xFFFF); // BLTAFWM
    amiga.poke_word(0x00DF_F046, 0xFFFF); // BLTALWM
    amiga.poke_word(0x00DF_F050, (src >> 16) as u16); // BLTAPTH
    amiga.poke_word(0x00DF_F052, (src & 0xFFFF) as u16); // BLTAPTL
    amiga.poke_word(0x00DF_F054, (dst >> 16) as u16); // BLTDPTH
    amiga.poke_word(0x00DF_F056, (dst & 0xFFFF) as u16); // BLTDPTL
    amiga.poke_word(0x00DF_F05C, 1); // BLTSIZV: 1 row
    amiga.poke_word(0x00DF_F05E, 1); // BLTSIZH: 1 word — starts the blit

    assert_ne!(
        amiga.read_word(DMACONR) & BBUSY,
        0,
        "BBUSY must be set after BLTSIZH (ECS large-blit start)"
    );

    let ticks = run_to_idle(&mut amiga);
    assert!(ticks > 0, "blit must take real chip cycles");
    assert_eq!(chip_word(&amiga, dst), 0xBEEF, "A→D copy must land");
    assert_ne!(
        amiga.read_word(INTREQR) & INT_BLIT,
        0,
        "INT_BLIT must be raised on completion"
    );
}

#[test]
fn ecs_legacy_bltsize_start_path_also_works() {
    // Legacy $058 BLTSIZE start path still works on ECS.
    let mut amiga = machine();
    let src = 0x0004_0000;
    let dst = 0x0005_0000;
    amiga.poke_word(src, 0x1234);

    amiga.poke_word(0x00DF_F096, 0x8000 | 0x0200 | 0x0040);
    amiga.poke_word(0x00DF_F040, 0x0900 | 0x00F0);
    amiga.poke_word(0x00DF_F042, 0x0000);
    amiga.poke_word(0x00DF_F044, 0xFFFF);
    amiga.poke_word(0x00DF_F046, 0xFFFF);
    amiga.poke_word(0x00DF_F050, (src >> 16) as u16);
    amiga.poke_word(0x00DF_F052, (src & 0xFFFF) as u16);
    amiga.poke_word(0x00DF_F054, (dst >> 16) as u16);
    amiga.poke_word(0x00DF_F056, (dst & 0xFFFF) as u16);
    amiga.poke_word(0x00DF_F058, (1 << 6) | 1); // BLTSIZE 1×1

    run_to_idle(&mut amiga);
    assert_eq!(chip_word(&amiga, dst), 0x1234);
}

#[test]
fn ecs_large_blit_wider_than_legacy_field_does_not_wrap() {
    // Regression for #36 end-to-end: a 100-word-wide D-only fill driven
    // through the ECS BLTSIZV/BLTSIZH path must write all 100 words, not
    // wrap to 100 & 0x3F = 36 at the legacy 6-bit width field.
    let mut amiga = machine();
    let dst = 0x0003_0000;

    amiga.poke_word(0x00DF_F096, 0x8000 | 0x0200 | 0x0040); // DMAEN | BLTEN
    amiga.poke_word(0x00DF_F040, 0x0100 | 0x00FF); // BLTCON0: USED + minterm $FF (D := 1)
    amiga.poke_word(0x00DF_F042, 0x0000); // BLTCON1
    amiga.poke_word(0x00DF_F044, 0xFFFF); // BLTAFWM
    amiga.poke_word(0x00DF_F046, 0xFFFF); // BLTALWM
    amiga.poke_word(0x00DF_F066, 0x0000); // BLTDMOD = 0 (contiguous)
    amiga.poke_word(0x00DF_F054, (dst >> 16) as u16); // BLTDPTH
    amiga.poke_word(0x00DF_F056, (dst & 0xFFFF) as u16); // BLTDPTL
    amiga.poke_word(0x00DF_F05C, 1); // BLTSIZV: 1 row
    amiga.poke_word(0x00DF_F05E, 100); // BLTSIZH: 100 words — starts the blit

    run_to_idle(&mut amiga);
    assert_eq!(chip_word(&amiga, dst + 63 * 2), 0xFFFF, "word 63 written");
    assert_eq!(
        chip_word(&amiga, dst + 64 * 2),
        0xFFFF,
        "word 64 written — legacy 6-bit width would have wrapped this away"
    );
    assert_eq!(chip_word(&amiga, dst + 99 * 2), 0xFFFF, "word 99 written");
    assert_eq!(
        chip_word(&amiga, dst + 100 * 2),
        0x0000,
        "word 100 not written"
    );
}
