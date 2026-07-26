//! End-to-end coverage for the original-Agnus vertical display-window
//! flip-flop and its interaction with a running DDF sequence.

use machine_commodore_amiga_ocs::{AmigaOcs, OriginalAgnusRevision, RamConfig};

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

fn parked_a1000_bootstrap_rom() -> Vec<u8> {
    let mut rom = vec![0u8; 64 * 1024];
    rom[0] = 0x11;
    rom[1] = 0x11;
    rom[2] = 0x4E; // JMP
    rom[3] = 0xF9;
    rom[4..8].copy_from_slice(&0x00F8_0008u32.to_be_bytes());
    rom[8] = 0x60; // BRA.S
    rom[9] = 0xFE; // -2: branch to self
    rom
}

fn advance_to_line(amiga: &mut AmigaOcs, target: u16) {
    let mut guard = 0;
    while amiga.agnus().vpos < target && guard < 200_000 {
        amiga.tick();
        guard += 1;
    }
    assert!(guard < 200_000, "beam did not reach line {target:#05x}");
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

fn advance_to_next_field(amiga: &mut AmigaOcs) {
    let vbl_count = amiga.agnus().vbl_count;
    let mut guard = 0;
    while amiga.agnus().vbl_count == vbl_count && guard < 2_000 {
        amiga.tick();
        guard += 1;
    }
    assert!(guard < 2_000, "beam did not enter the next field");
    assert_eq!(amiga.agnus().vpos, 0);
    assert_eq!(amiga.agnus().hpos, 0);
}

fn active_hires_run() -> AmigaOcs {
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

    advance_to_line(&mut amiga, 0x00B0);
    advance_to_hpos(&mut amiga, 0x0040);
    assert!(amiga.agnus().vertical_diw_active());
    assert_eq!(amiga.agnus().ddf_start_match(), Some(0x0038));
    amiga
}

#[test]
fn vertical_reopen_cannot_resume_an_aborted_run_but_future_ddfstart_can() {
    let mut amiga = active_hires_run();

    amiga.poke_word(DIWSTOP, 0xB0C1);
    advance_to_hpos(&mut amiga, 0x0048);
    assert!(!amiga.agnus().vertical_diw_active());
    assert!(amiga.agnus().ocs_ddf_run_aborted());
    assert_eq!(amiga.agnus().ddf_start_match(), Some(0x0038));
    let pointers_after_close = amiga.agnus().bpl_pt;

    amiga.poke_word(DIWSTOP, 0xF0C1);
    advance_to_hpos(&mut amiga, 0x0050);
    assert!(
        !amiga.agnus().vertical_diw_active(),
        "restoring register geometry cannot reconstruct the hidden latch",
    );
    assert_eq!(amiga.agnus().bpl_pt, pointers_after_close);

    amiga.poke_word(DIWSTRT, 0xB081);
    advance_to_hpos(&mut amiga, 0x0058);
    assert!(amiga.agnus().vertical_diw_active());
    assert!(amiga.agnus().ocs_ddf_run_aborted());
    assert_eq!(amiga.agnus().ddf_start_match(), Some(0x0038));
    assert_eq!(
        amiga.agnus().bpl_pt,
        pointers_after_close,
        "vertical reopening alone must not resume the stale DDF origin",
    );

    amiga.poke_word(DDFSTRT, 0x0080);
    advance_to_hpos(&mut amiga, 0x0080);
    assert_eq!(amiga.agnus().ddf_start_match(), Some(0x0080));
    assert!(!amiga.agnus().ocs_ddf_run_aborted());

    advance_to_hpos(&mut amiga, 0x0088);
    assert_ne!(
        amiga.agnus().bpl_pt,
        pointers_after_close,
        "a later eligible DDFSTRT must establish a fresh fetching phase",
    );
}

#[test]
fn a1000_and_later_original_agnus_use_their_installed_hard_blank_lines() {
    let a1000_ram = RamConfig {
        chip_kb: 256,
        slow_kb: 0,
        fast_kb: 0,
    };
    let mut a1000 = AmigaOcs::with_a1000_bootstrap_rom(parked_a1000_bootstrap_rom(), a1000_ram);
    let mut later = AmigaOcs::with_ram_config(parked_cpu_rom(), RamConfig::bare());

    for amiga in [&mut a1000, &mut later] {
        amiga.poke_word(DIWSTRT, 0xF081);
        amiga.poke_word(DIWSTOP, 0xE0C1);
        advance_to_line(amiga, 0x00F0);
        assert!(amiga.agnus().vertical_diw_active());
        amiga.poke_word(DIWSTRT, 0x0081);
        assert!(amiga.agnus().vertical_diw_active());
    }

    let final_line = later.agnus().lines_per_frame - 1;
    advance_to_line(&mut a1000, final_line);
    advance_to_line(&mut later, final_line);
    assert!(
        a1000.agnus().vertical_diw_active(),
        "A1000 must remain open on the final physical field line",
    );
    assert!(
        !later.agnus().vertical_diw_active(),
        "later original Agnus must hard-close on the final field line",
    );

    advance_to_next_field(&mut a1000);
    advance_to_next_field(&mut later);
    assert!(
        !a1000.agnus().vertical_diw_active(),
        "A1000 line-zero force-off must beat the matching VSTART",
    );
    assert!(
        later.agnus().vertical_diw_active(),
        "later original Agnus must allow the line-zero VSTART to reopen",
    );
}

#[test]
fn ntsc_builders_select_distinct_original_agnus_revisions_with_shared_vposr_id() {
    let a1000_ram = RamConfig {
        chip_kb: 256,
        slow_kb: 0,
        fast_kb: 0,
    };
    let a1000 = AmigaOcs::with_a1000_bootstrap_rom_ntsc(parked_a1000_bootstrap_rom(), a1000_ram);
    let later = AmigaOcs::with_ram_config_ntsc(parked_cpu_rom(), RamConfig::bare());

    assert_eq!(
        a1000.agnus().original_revision(),
        OriginalAgnusRevision::A1000,
    );
    assert_eq!(
        later.agnus().original_revision(),
        OriginalAgnusRevision::Later,
    );
    assert_eq!(
        a1000.agnus().vposr() & 0x7F00,
        later.agnus().vposr() & 0x7F00,
        "NTSC 8361 and 8370 share VPOSR identity despite different hard-blank timing",
    );
}
