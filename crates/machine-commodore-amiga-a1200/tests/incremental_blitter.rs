//! End-to-end wiring of the incremental blitter (#31) through the A1200
//! (AGA) machine tick loop, via the ECS/AGA large-blit start path
//! (BLTSIZV/BLTSIZH at $05C/$05E).
//!
//! The blit drains one DMA op per granted CCK, BBUSY stays asserted in
//! DMACONR until it finishes, and INT_BLIT fires on completion.

use machine_commodore_amiga_a1200::AmigaA1200;

const DMACONR: u32 = 0x00DF_F002;
const INTREQR: u32 = 0x00DF_F01E;
const BBUSY: u16 = 0x4000;
const INT_BLIT: u16 = 0x0040;

fn machine() -> AmigaA1200 {
    AmigaA1200::new(vec![0u8; 512 * 1024])
}

fn chip_word(amiga: &AmigaA1200, addr: u32) -> u16 {
    let hi = u16::from(amiga.read_chip_ram_byte(addr));
    let lo = u16::from(amiga.read_chip_ram_byte(addr + 1));
    (hi << 8) | lo
}

#[test]
fn aga_blit_drains_over_ccks_sets_bbusy_then_completes() {
    let mut amiga = machine();
    let src = 0x0002_0000;
    let dst = 0x0003_0000;
    amiga.poke_word(src, 0xCAFE);

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
        "BBUSY must be set after BLTSIZH"
    );

    let mut ticks = 0u32;
    while amiga.read_word(DMACONR) & BBUSY != 0 {
        amiga.tick();
        ticks += 1;
        assert!(ticks < 100_000, "blit never completed (BBUSY stuck)");
    }

    assert!(ticks > 0, "blit must take real chip cycles");
    assert_eq!(chip_word(&amiga, dst), 0xCAFE, "A→D copy must land");
    assert_ne!(
        amiga.read_word(INTREQR) & INT_BLIT,
        0,
        "INT_BLIT must be raised on completion"
    );
}
