//! Alice programmable beam timing through the A1200 register bus.

use machine_commodore_amiga_a1200::AmigaA1200;

fn parked_cpu_rom() -> Vec<u8> {
    let mut rom = vec![0u8; 256 * 1024];
    rom[0..4].copy_from_slice(&0x0008_0000u32.to_be_bytes());
    rom[4..8].copy_from_slice(&0x00F8_0008u32.to_be_bytes());
    rom[8] = 0x60; // BRA.S
    rom[9] = 0xFE; // -2
    rom
}

#[test]
fn a1200_machine_uses_programmed_beam_totals() {
    let mut amiga = AmigaA1200::new(parked_cpu_rom());

    // Alice inherits the ECS programmable beam counter registers.
    amiga.poke_word(0x00DF_F1C0, 3);
    amiga.poke_word(0x00DF_F1C8, 1);
    amiga.poke_word(0x00DF_F1DC, 0x0080);

    for _ in 0..8 {
        amiga.tick();
    }
    assert_eq!((amiga.agnus().vpos, amiga.agnus().hpos), (1, 0));

    for _ in 0..8 {
        amiga.tick();
    }
    assert_eq!((amiga.agnus().vpos, amiga.agnus().hpos), (0, 0));
}
