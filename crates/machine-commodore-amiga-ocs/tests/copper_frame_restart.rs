//! Automatic Copper restart timing.
//!
//! Fixed-sync A500-class hardware reloads the Copper from `COP1LC`
//! when the beam enters line zero. CIA-A's later `/VSYNC`-derived TOD
//! event must not be mistaken for that restart.

use machine_commodore_amiga_ocs::AmigaOcs;

const COP1LC: u32 = 0x0000_1000;
const COP2LC: u32 = 0x0000_2000;

fn parked_cpu_rom() -> Vec<u8> {
    let mut rom = vec![0u8; 256 * 1024];
    rom[0..4].copy_from_slice(&0x0008_0000u32.to_be_bytes());
    rom[4..8].copy_from_slice(&0x00F8_0008u32.to_be_bytes());
    rom[8] = 0x60; // BRA.S
    rom[9] = 0xFE; // -2
    rom
}

fn run_until_position(amiga: &mut AmigaOcs, vpos: u16, hpos: u16) {
    for _ in 0..200_000 {
        if amiga.agnus().vpos == vpos && amiga.agnus().hpos == hpos {
            return;
        }
        amiga.tick();
    }
    panic!("beam did not reach position ({vpos},{hpos})");
}

#[test]
fn copper_restarts_at_frame_wrap_not_cia_a_tod_event() {
    let mut amiga = AmigaOcs::new(parked_cpu_rom());

    // Keep Copper DMA disabled so only explicit and automatic jump
    // strobes can change the program counter.
    amiga.poke_word(0x00DF_F080, (COP1LC >> 16) as u16);
    amiga.poke_word(0x00DF_F082, COP1LC as u16);
    amiga.poke_word(0x00DF_F084, (COP2LC >> 16) as u16);
    amiga.poke_word(0x00DF_F086, COP2LC as u16);
    amiga.poke_word(0x00DF_F08A, 0);
    assert_eq!(amiga.copper().pc, COP2LC);

    // PAL /VSYNC deassertion is represented by the CIA-A TOD event at
    // line 5, hpos 84. It is not the automatic Copper restart.
    run_until_position(&mut amiga, 5, 84);
    assert_eq!(
        amiga.copper().pc,
        COP2LC,
        "CIA-A TOD timing must not reload the Copper"
    );

    let initial_frame = amiga.agnus().vbl_count;
    while amiga.agnus().vbl_count == initial_frame {
        amiga.tick();
    }

    assert_eq!((amiga.agnus().vpos, amiga.agnus().hpos), (0, 0));
    assert_eq!(
        amiga.copper().pc,
        COP1LC,
        "frame wrap should issue the automatic COP1LC restart"
    );
}
