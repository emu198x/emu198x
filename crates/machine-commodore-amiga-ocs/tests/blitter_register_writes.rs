//! Writes to an active blitter remain ordinary scheduled chipset writes.
//!
//! The machine must not finish an entire blit inside a CPU or Copper register
//! dispatch. Software that reprograms the blitter is responsible for waiting
//! for the preceding operation.

use machine_commodore_amiga_ocs::AmigaOcs;

const BLTCON0: u32 = 0x00DF_F040;
const BLTSIZE: u32 = 0x00DF_F058;
const COPCON: u32 = 0x00DF_F02E;
const COP1LCH: u32 = 0x00DF_F080;
const COP1LCL: u32 = 0x00DF_F082;
const COPJMP1: u32 = 0x00DF_F088;
const DMACON: u32 = 0x00DF_F096;
const INT_BLIT: u16 = 0x0040;

fn parked_kickstart() -> Vec<u8> {
    let mut rom = vec![0u8; 512 * 1024];
    rom[0..4].copy_from_slice(&0x0008_0000u32.to_be_bytes());
    rom[4..8].copy_from_slice(&0x00F8_0008u32.to_be_bytes());
    rom[8] = 0x60;
    rom[9] = 0xFE;
    rom
}

#[test]
fn copper_move_changes_a_live_register_without_draining_the_blit() {
    let mut amiga = AmigaOcs::new(parked_kickstart());

    // Arm an internal operation but leave BLTEN clear. It remains at the
    // start boundary while the independently enabled Copper executes.
    amiga.poke_word(BLTCON0, 0);
    amiga.poke_word(BLTSIZE, (2 << 6) | 2);
    assert!(amiga.agnus().blitter_busy);
    assert_eq!(amiga.agnus().blitter_startup_ccks_remaining(), 2);

    // CDANG permits a Copper MOVE below $080. The list changes BLTCON0 and
    // then parks forever.
    amiga.poke_word(0x0000_1000, 0x0040);
    amiga.poke_word(0x0000_1002, 0x1234);
    amiga.poke_word(0x0000_1004, 0xFFFF);
    amiga.poke_word(0x0000_1006, 0xFFFE);
    amiga.poke_word(COPCON, 0x0002);
    amiga.poke_word(COP1LCH, 0);
    amiga.poke_word(COP1LCL, 0x1000);
    amiga.poke_word(COPJMP1, 0);
    amiga.poke_word(DMACON, 0x8000 | 0x0200 | 0x0080);

    let mut guard = 0;
    while amiga.agnus().bltcon0 != 0x1234 {
        amiga.tick();
        guard += 1;
        assert!(guard < 1_000, "Copper never performed its BLTCON0 MOVE");
    }

    assert!(amiga.agnus().blitter_busy);
    assert_eq!(amiga.agnus().blitter_startup_ccks_remaining(), 2);
    assert_eq!(amiga.intreq() & INT_BLIT, 0);
    assert!(
        amiga
            .debug_copper_move_log
            .iter()
            .any(|&(_, _, _, reg, value)| reg == 0x0040 && value == 0x1234),
    );
}

#[test]
fn second_bltsize_replaces_the_scheduled_operation_without_finishing_it() {
    let mut amiga = AmigaOcs::new(parked_kickstart());

    amiga.poke_word(BLTCON0, 0);
    amiga.poke_word(BLTSIZE, (4 << 6) | 4);
    assert!(amiga.agnus().blitter_busy);
    assert_eq!(amiga.agnus().blitter_ccks_remaining, 16);
    assert_eq!(amiga.debug_blit_starts, 1);

    // The second start replaces the queued 4×4 internal operation. It must
    // not retire the first operation, emit its interrupt or advance time.
    let tick_before = amiga.tick_count();
    let beam_before = (amiga.agnus().vpos, amiga.agnus().hpos);
    amiga.poke_word(BLTSIZE, (1 << 6) | 1);

    assert!(amiga.agnus().blitter_busy);
    assert_eq!(amiga.agnus().blitter_startup_ccks_remaining(), 2);
    assert_eq!(amiga.agnus().blitter_ccks_remaining, 1);
    assert_eq!(amiga.debug_blit_starts, 2);
    assert_eq!(amiga.intreq() & INT_BLIT, 0);
    assert_eq!(amiga.tick_count(), tick_before);
    assert_eq!((amiga.agnus().vpos, amiga.agnus().hpos), beam_before);
}
