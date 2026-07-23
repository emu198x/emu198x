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
