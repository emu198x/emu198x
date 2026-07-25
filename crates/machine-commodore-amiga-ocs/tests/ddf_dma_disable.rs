//! End-to-end coverage for original-Agnus DDF run termination when
//! effective bitplane DMA is disabled.

use machine_commodore_amiga_ocs::{AmigaOcs, RamConfig};

const CUSTOM_BASE: u32 = 0x00DF_F000;
const DIWSTRT: u32 = CUSTOM_BASE + 0x08E;
const DIWSTOP: u32 = CUSTOM_BASE + 0x090;
const DDFSTRT: u32 = CUSTOM_BASE + 0x092;
const DDFSTOP: u32 = CUSTOM_BASE + 0x094;
const DMACON: u32 = CUSTOM_BASE + 0x096;
const BPLCON0: u32 = CUSTOM_BASE + 0x100;
const BPL_POINTER_REGS: [(u32, u32); 4] = [
    (CUSTOM_BASE + 0x0E0, CUSTOM_BASE + 0x0E2),
    (CUSTOM_BASE + 0x0E4, CUSTOM_BASE + 0x0E6),
    (CUSTOM_BASE + 0x0E8, CUSTOM_BASE + 0x0EA),
    (CUSTOM_BASE + 0x0EC, CUSTOM_BASE + 0x0EE),
];
const BITPLANE_BASES: [u32; 4] = [0x0001_0000, 0x0001_2000, 0x0001_4000, 0x0001_6000];

fn parked_cpu_rom() -> Vec<u8> {
    let mut rom = vec![0u8; 256 * 1024];
    rom[0..4].copy_from_slice(&0x0008_0000u32.to_be_bytes());
    rom[4..8].copy_from_slice(&0x00F8_0008u32.to_be_bytes());
    rom[8] = 0x60; // BRA.S
    rom[9] = 0xFE; // -2: branch to self
    rom
}

fn advance_to_line(amiga: &mut AmigaOcs, target: u16) {
    let mut guard = 0;
    while amiga.agnus().vpos < target && guard < 100_000 {
        amiga.tick();
        guard += 1;
    }
    assert!(guard < 100_000, "beam did not reach line {target:#05x}");
    assert_eq!(amiga.agnus().hpos, 0);
}

fn advance_to_hpos(amiga: &mut AmigaOcs, target: u16) {
    let line = amiga.agnus().vpos;
    let mut guard = 0;
    while amiga.agnus().hpos < target && guard < 1_000 {
        amiga.tick();
        guard += 1;
    }
    assert!(guard < 1_000, "beam did not reach hpos {target:#05x}");
    assert_eq!(amiga.agnus().vpos, line, "beam crossed a line boundary");
    assert_eq!(amiga.agnus().hpos, target);
}

#[test]
fn reenabled_bitplane_dma_does_not_advance_an_aborted_ocs_run() {
    let mut amiga = AmigaOcs::with_ram_config(parked_cpu_rom(), RamConfig::bare());
    amiga.poke_word(DIWSTRT, 0x3081);
    amiga.poke_word(DIWSTOP, 0xF0C1);
    amiga.poke_word(DDFSTRT, 0x0038);
    amiga.poke_word(DDFSTOP, 0x00D0);
    amiga.poke_word(BPLCON0, 0xC200); // hires, four planes, colour enabled
    for ((high, low), pointer) in BPL_POINTER_REGS.into_iter().zip(BITPLANE_BASES) {
        amiga.poke_word(high, (pointer >> 16) as u16);
        amiga.poke_word(low, pointer as u16);
    }
    amiga.poke_word(DMACON, 0x8300); // SETCLR | DMAEN | BPLEN

    advance_to_line(&mut amiga, 0x0030);
    advance_to_hpos(&mut amiga, 0x0040);
    assert_eq!(amiga.agnus().ddf_start_match(), Some(0x0038));

    amiga.poke_word(DMACON, 0x0100); // clear BPLEN
    advance_to_hpos(&mut amiga, 0x0048);
    let pointers_after_disable = amiga.agnus().bpl_pt;
    amiga.poke_word(DMACON, 0x8100); // set BPLEN
    advance_to_hpos(&mut amiga, 0x00D8);

    assert!(amiga.agnus().ocs_ddf_run_aborted());
    assert_eq!(amiga.agnus().ddf_start_match(), Some(0x0038));
    assert_eq!(amiga.agnus().ddf_stop_match(), None);
    assert_eq!(amiga.agnus().ddf_fetch_end(), None);
    assert_eq!(
        amiga.agnus().bpl_pt,
        pointers_after_disable,
        "re-enabling BPLEN must not advance pointers from the stale fetch origin",
    );
}
