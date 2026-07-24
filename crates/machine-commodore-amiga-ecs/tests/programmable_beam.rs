//! ECS programmable beam timing through the machine register bus.

use machine_commodore_amiga_ecs::AmigaEcs;

fn parked_cpu_rom() -> Vec<u8> {
    let mut rom = vec![0u8; 256 * 1024];
    rom[0..4].copy_from_slice(&0x0008_0000u32.to_be_bytes());
    rom[4..8].copy_from_slice(&0x00F8_0008u32.to_be_bytes());
    rom[8] = 0x60; // BRA.S
    rom[9] = 0xFE; // -2
    rom
}

#[test]
fn ecs_machine_uses_programmed_beam_totals() {
    let mut amiga = AmigaEcs::new(parked_cpu_rom());

    // HTOTAL=3 gives four CCKs per line; VTOTAL=1 gives two lines.
    // BEAMCON0.VARBEAMEN selects the programmable counters.
    amiga.poke_word(0x00DF_F1C0, 3);
    amiga.poke_word(0x00DF_F1C8, 1);
    amiga.poke_word(0x00DF_F1DC, 0x00A0); // VARBEAMEN | PAL

    for _ in 0..8 {
        amiga.tick();
    }
    assert_eq!((amiga.agnus().vpos, amiga.agnus().hpos), (1, 0));

    for _ in 0..8 {
        amiga.tick();
    }
    assert_eq!((amiga.agnus().vpos, amiga.agnus().hpos), (0, 0));
    assert_eq!(amiga.agnus().vbl_count, 1);
}

#[test]
fn ecs_machine_uses_programmed_ntsc_short_and_long_lines() {
    let mut amiga = AmigaEcs::new(parked_cpu_rom());
    amiga.poke_word(0x00DF_F1C0, 1); // Two CCK short line.
    amiga.poke_word(0x00DF_F1C8, 7);
    amiga.poke_word(0x00DF_F1DC, 0x0080); // VARBEAMEN, PAL clear.

    for _ in 0..4 {
        amiga.tick();
    }
    assert_eq!((amiga.agnus().vpos, amiga.agnus().hpos), (1, 0));
    assert!(amiga.agnus().lol);

    for _ in 0..4 {
        amiga.tick();
    }
    assert_eq!((amiga.agnus().vpos, amiga.agnus().hpos), (1, 2));

    for _ in 0..2 {
        amiga.tick();
    }
    assert_eq!((amiga.agnus().vpos, amiga.agnus().hpos), (2, 0));
    assert!(!amiga.agnus().lol);
}

#[test]
fn programmed_vbstop_fetches_control_before_sprite_pixels_reach_the_framebuffer() {
    let mut amiga = AmigaEcs::new(parked_cpu_rom());

    // Keep line 50 inside the display window and make sprite colour 1 red.
    amiga.poke_word(0x00DF_F08E, 0x2C81);
    amiga.poke_word(0x00DF_F090, 0xF4C1);
    amiga.poke_word(0x00DF_F1A2, 0x0F00);

    // Move the blank-stop/control-load event from fixed PAL line 25 to
    // programmed line 40. The programmed beam counter itself remains off.
    amiga.poke_word(0x00DF_F1CC, 300); // VBSTRT
    amiga.poke_word(0x00DF_F1CE, 40); // VBSTOP
    amiga.poke_word(0x00DF_F1DC, 0x1020); // VARVBEN | PAL

    // Sprite 0 control at $2000, followed by opaque colour-1 data.
    amiga.poke_word(0x0000_2000, 0x3264); // VSTART=50, HSTART=200
    amiga.poke_word(0x0000_2002, 0x3C00); // VSTOP=60
    for line in 0..16u32 {
        amiga.poke_word(0x0000_2004 + line * 4, 0xFFFF);
        amiga.poke_word(0x0000_2006 + line * 4, 0x0000);
    }
    amiga.poke_word(0x00DF_F120, 0x0000);
    amiga.poke_word(0x00DF_F122, 0x2000);

    // Traverse VBSTRT and wrap into the following field before enabling
    // sprite DMA. Programmable blanking is an edge-driven latch: writing a
    // range around the current line does not reconstruct its historical
    // level.
    let mut guard = 0;
    while amiga.agnus().vpos < 300 && guard < 4_000_000 {
        amiga.tick();
        guard += 1;
    }
    amiga.poke_word(0x00DF_F096, 0x8220); // SETCLR | DMAEN | SPREN
    let field = amiga.agnus().vbl_count;
    while amiga.agnus().vbl_count == field && guard < 4_000_000 {
        amiga.tick();
        guard += 1;
    }

    while amiga.agnus().vpos < 40 && guard < 4_000_000 {
        amiga.tick();
        guard += 1;
    }
    assert_eq!(
        amiga.agnus().spr_pt[0],
        0x0000_2000,
        "the fixed PAL line must not fetch control while VARVBEN is selected"
    );

    while amiga.agnus().vpos < 41 && guard < 4_000_000 {
        amiga.tick();
        guard += 1;
    }
    assert_eq!(amiga.agnus().spr_pt[0], 0x0000_2004);
    assert_eq!(amiga.agnus().sprite_vstart(0), 50);
    assert_eq!(amiga.agnus().sprite_vstop(0), 60);

    while amiga.agnus().vpos < 62 && guard < 4_000_000 {
        amiga.tick();
        guard += 1;
    }
    assert!(
        amiga.denise().ocs.sprite_pixels_rendered(0) > 0,
        "the programmed control load must drive the Denise sprite pipeline"
    );
    assert!(
        amiga.denise().framebuffer().contains(&0xFFFF_0000),
        "opaque sprite data must reach the board framebuffer as COLOR17"
    );
}
