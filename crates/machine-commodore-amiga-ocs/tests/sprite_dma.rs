//! Integration: hardware sprite DMA (gap #162).
//!
//! Agnus fetches a sprite's control + data words from chip RAM at SPRxPT
//! and delivers them to Denise. This sets up sprite 0 the normal (DMA)
//! way — control words in chip RAM, SPR0PT pointed at them, SPREN
//! enabled — runs past the reset line, and confirms Agnus latched
//! VSTART/VSTOP from chip RAM and activated the sprite. The earlier
//! per-line board implementation advanced the pointer every line and
//! desynced the control/data stream, so DMA-driven sprites (including
//! the Workbench mouse pointer) never displayed.

use machine_commodore_amiga_ocs::AmigaOcs;

/// A minimal ROM whose reset vector parks the CPU in an infinite
/// `BRA.S *` self-loop, so it never executes the blank ROM as garbage
/// and writes spurious custom registers during the test.
fn parked_cpu_rom() -> Vec<u8> {
    let mut rom = vec![0u8; 256 * 1024];
    rom[0..4].copy_from_slice(&0x0008_0000u32.to_be_bytes()); // initial SSP
    rom[4..8].copy_from_slice(&0x00F8_0008u32.to_be_bytes()); // initial PC -> $F80008
    rom[8] = 0x60; // BRA.S
    rom[9] = 0xFE; // displacement -2 -> branch to self
    rom
}

#[test]
fn dma_sprite_control_words_are_fetched_and_sprite_activates() {
    let mut amiga = AmigaOcs::new(parked_cpu_rom());

    // Sprite 0 control block + a few data lines in chip RAM at $2000:
    //   SPR0POS: VSTART low byte (bits 15-8) = 30 -> 0x1E00
    //   SPR0CTL: VSTOP  low byte (bits 15-8) = 40 -> 0x2800
    amiga.poke_word(0x2000, 0x1E00); // POS  vstart = 30
    amiga.poke_word(0x2002, 0x2800); // CTL  vstop  = 40
    for line in 0..16u32 {
        amiga.poke_word(0x2004 + line * 4, 0xF00F); // DATA
        amiga.poke_word(0x2006 + line * 4, 0x0FF0); // DATB
    }

    // SPR0PT = $0000_2000 (SPR0PTH $120 high word, SPR0PTL $122 low word).
    amiga.poke_word(0x00DF_F120, 0x0000);
    amiga.poke_word(0x00DF_F122, 0x2000);

    // Enable sprite DMA: DMACON SET | DMAEN | SPREN.
    amiga.poke_word(0x00DF_F096, 0x8000 | 0x0200 | 0x0020);

    // Run until the beam reaches the sprite's display region. The
    // reset-line control fetch (latches VSTART/VSTOP from chip RAM) and
    // the VSTART activation happen by line ~30.
    let mut guard = 0;
    while amiga.agnus().vpos < 33 && guard < 4_000_000 {
        amiga.tick();
        guard += 1;
    }

    assert_eq!(
        amiga.agnus().sprite_vstart(0),
        30,
        "VSTART latched from the DMA-fetched SPR0POS"
    );
    assert_eq!(
        amiga.agnus().sprite_vstop(0),
        40,
        "VSTOP latched from the DMA-fetched SPR0CTL"
    );
    assert!(
        amiga.agnus().sprite_dma_on(0),
        "sprite DMA is active between VSTART and VSTOP"
    );
    assert!(
        amiga.agnus().spr_pt[0] > 0x2004,
        "SPR0PT advanced past the control words as data was fetched"
    );
}
