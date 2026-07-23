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

use machine_commodore_amiga_ocs::{AmigaOcs, RamConfig};

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

#[test]
fn ntsc_sprite_control_fetches_on_line_20_and_data_starts_on_line_21() {
    let mut amiga = AmigaOcs::with_ram_config_ntsc(parked_cpu_rom(), RamConfig::bare());

    amiga.poke_word(0x2000, 0x1500); // POS: VSTART = 21
    amiga.poke_word(0x2002, 0x1E00); // CTL: VSTOP = 30
    amiga.poke_word(0x2004, 0xFFFF); // first DATA word
    amiga.poke_word(0x2006, 0x0000); // first DATB word
    amiga.poke_word(0x00DF_F120, 0x0000);
    amiga.poke_word(0x00DF_F122, 0x2000);
    amiga.poke_word(0x00DF_F096, 0x8000 | 0x0200 | 0x0020);

    let mut guard = 0;
    while amiga.agnus().vpos < 22 && guard < 4_000_000 {
        amiga.tick();
        guard += 1;
    }

    assert_eq!(amiga.agnus().sprite_vstart(0), 21);
    assert_eq!(amiga.agnus().sprite_vstop(0), 30);
    assert!(
        amiga.agnus().sprite_dma_on(0),
        "the first NTSC data line activates at VSTART 21"
    );
    assert!(
        amiga.agnus().spr_pt[0] > 0x2004,
        "line-20 control and line-21 data fetches advance SPR0PT"
    );
}

/// End-to-end: a DMA-driven sprite must actually composite *pixels* into
/// the framebuffer, not merely fetch its words. This is the test that was
/// missing when gap #162 first shipped — Agnus fetched control + data
/// correctly, but the board fed Denise's sprite comparator scroll-relative
/// pipeline coordinates instead of the absolute beam position, so the
/// comparator never matched and the Workbench pointer never drew. We open
/// the display window, place the sprite inside it the DMA way, and assert
/// Denise's per-sprite rendered-pixel counter is non-zero.
#[test]
fn dma_sprite_renders_pixels_into_the_display() {
    let mut amiga = AmigaOcs::new(parked_cpu_rom());

    // Open a wide display window: DIWSTRT vstart=0x2C(44)/hstart=0x81(129),
    // DIWSTOP vstop=0xF4(244)/hstop=0x1C1(449).
    amiga.poke_word(0x00DF_F08E, 0x2C81); // DIWSTRT
    amiga.poke_word(0x00DF_F090, 0xF4C1); // DIWSTOP

    // Sprite 0 the DMA way, positioned inside the window:
    //   SPR0POS = 0x3264 -> VSTART=0x32(50), HSTART=(0x64)<<1=200
    //   SPR0CTL = 0x3C00 -> VSTOP =0x3C(60), HSTART bit0=0, no ATTACH
    //   DATA=0xFFFF / DATB=0x0000 -> every pixel code 1 (opaque)
    amiga.poke_word(0x2000, 0x3264); // POS
    amiga.poke_word(0x2002, 0x3C00); // CTL
    for line in 0..16u32 {
        amiga.poke_word(0x2004 + line * 4, 0xFFFF); // DATA
        amiga.poke_word(0x2006 + line * 4, 0x0000); // DATB
    }

    amiga.poke_word(0x00DF_F120, 0x0000); // SPR0PTH
    amiga.poke_word(0x00DF_F122, 0x2000); // SPR0PTL
    amiga.poke_word(0x00DF_F096, 0x8000 | 0x0200 | 0x0020); // DMACON: DMAEN|SPREN

    // Run past the sprite's VSTOP line so all active lines have been drawn.
    let mut guard = 0;
    while amiga.agnus().vpos < 62 && guard < 4_000_000 {
        amiga.tick();
        guard += 1;
    }

    assert!(
        amiga.denise().ocs.sprite_pixels_rendered(0) > 0,
        "DMA-driven sprite 0 composited at least one pixel into the display \
         (got {})",
        amiga.denise().ocs.sprite_pixels_rendered(0)
    );
}

/// Regression (#455): a DMA sprite positioned by *direct* CPU/copper
/// writes to SPR0POS/SPR0CTL — with the chip-RAM control words left
/// zero — must still render. Blitz BASIC's `ShowSprite` works this way:
/// it leaves the sprite data header at 0 and writes the position
/// registers directly each frame. Direct SPRxPOS/CTL writes used to
/// reach Denise only, never Agnus's VSTART/VSTOP comparator, so the
/// sprite never activated and nothing drew. We reproduce that exact
/// shape — zero header, direct register writes applied after the
/// reset-line control fetch (as a per-frame copper would) — and assert
/// Denise composited pixels.
#[test]
fn sprite_positioned_by_direct_pos_ctl_writes_renders() {
    let mut amiga = AmigaOcs::new(parked_cpu_rom());

    // Open the same wide display window as the DMA-way test.
    amiga.poke_word(0x00DF_F08E, 0x2C81); // DIWSTRT
    amiga.poke_word(0x00DF_F090, 0xF4C1); // DIWSTOP

    // Sprite 0 data at $2000 with a ZERO control-word header (the Blitz
    // shape) followed by opaque image lines.
    amiga.poke_word(0x2000, 0x0000); // POS — left zero
    amiga.poke_word(0x2002, 0x0000); // CTL — left zero
    for line in 0..16u32 {
        amiga.poke_word(0x2004 + line * 4, 0xFFFF); // DATA
        amiga.poke_word(0x2006 + line * 4, 0x0000); // DATB
    }
    amiga.poke_word(0x00DF_F120, 0x0000); // SPR0PTH
    amiga.poke_word(0x00DF_F122, 0x2000); // SPR0PTL
    amiga.poke_word(0x00DF_F096, 0x8000 | 0x0200 | 0x0020); // DMACON: DMAEN|SPREN

    // Run past the reset line (VBL_END_LINE = 25), where the DMA control
    // fetch reads the zero header and pins VSTART/VSTOP at 0.
    let mut guard = 0;
    while amiga.agnus().vpos < 40 && guard < 4_000_000 {
        amiga.tick();
        guard += 1;
    }
    assert_eq!(
        amiga.agnus().sprite_vstart(0),
        0,
        "zero header pins VSTART at 0 until a register write positions it"
    );

    // Now position the sprite the Blitz way — direct register writes:
    //   SPR0POS = 0x3264 -> VSTART=0x32(50), HSTART=(0x64)<<1=200
    //   SPR0CTL = 0x3C00 -> VSTOP =0x3C(60)
    amiga.poke_word(0x00DF_F140, 0x3264); // SPR0POS
    amiga.poke_word(0x00DF_F142, 0x3C00); // SPR0CTL

    assert_eq!(
        amiga.agnus().sprite_vstart(0),
        50,
        "direct SPR0POS write must update Agnus's VSTART comparator"
    );
    assert_eq!(
        amiga.agnus().sprite_vstop(0),
        60,
        "direct SPR0CTL write must update Agnus's VSTOP comparator"
    );

    // Run past VSTOP so every active line has been composited.
    while amiga.agnus().vpos < 62 && guard < 8_000_000 {
        amiga.tick();
        guard += 1;
    }

    assert!(
        amiga.denise().ocs.sprite_pixels_rendered(0) > 0,
        "sprite positioned by direct SPR0POS/CTL writes composited at least \
         one pixel (got {})",
        amiga.denise().ocs.sprite_pixels_rendered(0)
    );
}
