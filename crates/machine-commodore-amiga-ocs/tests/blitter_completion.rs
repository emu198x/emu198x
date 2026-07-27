//! End-to-end observation of the pre-AGA blitter completion pipeline.
//!
//! Main finish, DMACONR BBUSY, Copper BFD and the final D write do not
//! share one edge. This test keeps the Copper parked on a BFD=0 WAIT,
//! withholds the final-D grant, and observes each boundary through the
//! normal OCS machine driver.

use machine_commodore_amiga_ocs::AmigaOcs;

const DMACONR: u32 = 0x00DF_F002;
const INTREQR: u32 = 0x00DF_F01E;
const BLTCON0: u32 = 0x00DF_F040;
const BLTCON1: u32 = 0x00DF_F042;
const BLTAFWM: u32 = 0x00DF_F044;
const BLTALWM: u32 = 0x00DF_F046;
const BLTDPTH: u32 = 0x00DF_F054;
const BLTDPTL: u32 = 0x00DF_F056;
const BLTSIZE: u32 = 0x00DF_F058;
const COP1LCH: u32 = 0x00DF_F080;
const COP1LCL: u32 = 0x00DF_F082;
const COPJMP1: u32 = 0x00DF_F088;
const DMACON: u32 = 0x00DF_F096;

const BBUSY: u16 = 0x4000;
const BZERO: u16 = 0x2000;
const INT_BLIT: u16 = 0x0040;

fn parked_kickstart() -> Vec<u8> {
    let mut rom = vec![0u8; 512 * 1024];
    rom[0..4].copy_from_slice(&0x0008_0000u32.to_be_bytes());
    rom[4..8].copy_from_slice(&0x00F8_0008u32.to_be_bytes());
    rom[8] = 0x60; // BRA.S *
    rom[9] = 0xFE;
    rom
}

fn chip_word(amiga: &AmigaOcs, addr: u32) -> u16 {
    let hi = u16::from(amiga.read_chip_ram_byte(addr));
    let lo = u16::from(amiga.read_chip_ram_byte(addr + 1));
    (hi << 8) | lo
}

fn tick_one_cck(amiga: &mut AmigaOcs) {
    let beam = (amiga.agnus().vpos, amiga.agnus().hpos);
    while (amiga.agnus().vpos, amiga.agnus().hpos) == beam {
        amiga.tick();
    }
}

fn install_matching_bfd0_wait(amiga: &mut AmigaOcs) {
    // WAIT for an already-satisfied beam position, but retain BFD=0 so
    // the Copper cannot proceed until its blitter-finished input clears.
    amiga.poke_word(0x0000_1000, 0x0001);
    amiga.poke_word(0x0000_1002, 0x7FFE);
    amiga.poke_word(0x0000_1004, 0x0180); // MOVE COLOR00, $0F00
    amiga.poke_word(0x0000_1006, 0x0F00);
    amiga.poke_word(0x0000_1008, 0xFFFF);
    amiga.poke_word(0x0000_100A, 0xFFFE);
    amiga.poke_word(COP1LCH, 0);
    amiga.poke_word(COP1LCL, 0x1000);
    amiga.poke_word(COPJMP1, 0);
}

fn start_one_word_d_fill(amiga: &mut AmigaOcs, destination: u32) {
    amiga.poke_word(BLTCON0, 0x01FF); // USED | D := $FFFF
    amiga.poke_word(BLTCON1, 0);
    amiga.poke_word(BLTAFWM, 0xFFFF);
    amiga.poke_word(BLTALWM, 0xFFFF);
    amiga.poke_word(BLTDPTH, (destination >> 16) as u16);
    amiga.poke_word(BLTDPTL, destination as u16);
    amiga.poke_word(BLTSIZE, (1 << 6) | 1);
}

#[test]
fn copper_bfd_and_final_d_follow_distinct_pre_aga_completion_edges() {
    const DESTINATION: u32 = 0x0003_0000;

    let mut amiga = AmigaOcs::new(parked_kickstart());
    install_matching_bfd0_wait(&mut amiga);
    start_one_word_d_fill(&mut amiga, DESTINATION);

    // Run the Copper while BLTEN is clear. Later original Agnus exposes
    // busy immediately, so the matching BFD=0 WAIT must park.
    amiga.poke_word(DMACON, 0x8280); // SETCLR | DMAEN | COPEN
    let mut guard = 0;
    while !amiga.copper().waiting {
        amiga.tick();
        guard += 1;
        assert!(guard < 1_000, "Copper never entered its BFD=0 WAIT");
    }
    assert_eq!(amiga.agnus().blitter_startup_ccks_remaining(), 2);
    assert_eq!(amiga.color(0), 0);

    // Admit blitter progress on unowned CPU/free cells. Nasty mode is
    // enabled initially so the completion boundary is independent of the
    // parked CPU's incidental chip accesses.
    amiga.poke_word(DMACON, 0x8440); // SETCLR | BLTPRI | BLTEN
    while amiga.agnus().blitter_completion_phase() != "final-result" {
        amiga.tick();
        guard += 1;
        assert!(guard < 2_000, "blitter never reached main finish");
    }

    // F: pre-AGA main finish has emitted INT_BLIT, but both busy
    // observers still report busy and the result/write tail is untouched.
    assert_ne!(amiga.read_word(INTREQR) & INT_BLIT, 0);
    assert_ne!(amiga.read_word(DMACONR) & BBUSY, 0);
    assert_ne!(amiga.read_word(DMACONR) & BZERO, 0);
    assert!(amiga.agnus().blitter_busy);
    assert!(
        !amiga.agnus().blitter_nasty_active(),
        "main finish releases nasty ownership before the serialized tail",
    );
    assert!(amiga.agnus().blitter_busy_copper());
    assert!(amiga.copper().waiting);
    assert_eq!(amiga.agnus().blitter_completion_ccks_remaining(), 2);
    assert_eq!(chip_word(&amiga, DESTINATION), 0);

    tick_one_cck(&mut amiga);

    // F+1: result generation clears BZERO without a bus grant. DMACONR
    // may now report idle, while the Copper's longer completion hold keeps
    // its BFD=0 WAIT parked and final D is still pending.
    assert_eq!(amiga.agnus().blitter_completion_phase(), "final-write");
    assert_eq!(amiga.agnus().blitter_completion_ccks_remaining(), 1);
    assert_eq!(amiga.read_word(DMACONR) & (BBUSY | BZERO), 0);
    assert!(amiga.agnus().blitter_busy);
    assert!(amiga.agnus().blitter_busy_copper());
    assert!(amiga.copper().waiting);
    assert_eq!(chip_word(&amiga, DESTINATION), 0);

    // Withhold the required final-D grant. On the next CCK the Copper's
    // observer reaches its first-idle edge, but the final write must remain
    // serialized in the blitter until BLTEN permits another progress grant.
    amiga.poke_word(DMACON, 0x0040); // clear BLTEN
    tick_one_cck(&mut amiga);

    assert_eq!(amiga.agnus().blitter_completion_phase(), "final-write");
    assert_eq!(amiga.agnus().blitter_completion_ccks_remaining(), 1);
    assert!(amiga.agnus().blitter_final_d_pending());
    assert!(amiga.agnus().blitter_busy);
    assert!(!amiga.agnus().blitter_busy_copper());
    assert!(!amiga.copper().waiting);
    assert!(!amiga.agnus().blitter_bus_used_this_cck());
    assert_eq!(chip_word(&amiga, DESTINATION), 0);

    amiga.poke_word(DMACON, 0x8040); // re-enable BLTEN without BLTPRI
    while amiga.agnus().blitter_busy {
        amiga.tick();
        guard += 1;
        assert!(guard < 3_000, "pending final D never received a grant");
    }

    assert!(
        amiga.agnus().blitter_bus_used_this_cck(),
        "a non-nasty final D must retain same-CCK blitter bus use",
    );
    assert_eq!(chip_word(&amiga, DESTINATION), 0xFFFF);
    assert!(!amiga.agnus().blitter_final_d_pending());

    while amiga.color(0) & 0x0FFF == 0 {
        amiga.tick();
        guard += 1;
        assert!(guard < 4_000, "Copper did not continue after BFD released");
    }
    assert_eq!(amiga.color(0) & 0x0FFF, 0x0F00);
}
